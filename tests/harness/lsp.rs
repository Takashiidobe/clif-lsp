use std::collections::VecDeque;
use std::time::Duration;

use clif_lsp::modes::{parse_edit, plain_text};
use clif_lsp::server::Backend;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf, split};
use tower_lsp_server::{LspService, Server};

const URI: &str = "file:///fixture.clif";
const TIMEOUT: Duration = Duration::from_secs(5);

struct Client {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    next_id: u64,
    version: i64,
    legend: Vec<String>,
    notifications: VecDeque<Value>,
}

impl Client {
    async fn send(&mut self, message: &Value) -> Result<(), String> {
        let body = message.to_string();
        let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        self.writer.write_all(frame.as_bytes()).await.map_err(|e| e.to_string())
    }

    async fn receive(&mut self) -> Result<Value, String> {
        let mut length = None;
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line).await.map_err(|e| e.to_string())? == 0 {
                return Err("server closed the connection".into());
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                length = Some(value.trim().parse::<usize>().map_err(|e| e.to_string())?);
            }
        }
        let mut body = vec![0; length.ok_or("frame without Content-Length")?];
        self.reader.read_exact(&mut body).await.map_err(|e| e.to_string())?;
        serde_json::from_slice(&body).map_err(|e| e.to_string())
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params})).await
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<String, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await?;
        tokio::time::timeout(TIMEOUT, async {
            loop {
                let message = self.receive().await?;
                match (message.get("method"), message.get("id")) {
                    (Some(_), Some(request_id)) => {
                        let reply = json!({"jsonrpc": "2.0", "id": request_id, "result": null});
                        self.send(&reply).await?;
                    }
                    (Some(_), None) => self.notifications.push_back(message),
                    (None, _) if message.get("id") == Some(&json!(id)) => return Ok(render_response(&message)),
                    (None, _) => {}
                }
            }
        })
        .await
        .map_err(|_| format!("timed out waiting for the response to `{method}`"))?
    }

    async fn await_notification(&mut self, method: &str) -> Result<String, String> {
        tokio::time::timeout(TIMEOUT, async {
            loop {
                if let Some(at) = self.notifications.iter().position(|n| n["method"] == method) {
                    return Ok(self.notifications.remove(at).unwrap()["params"].to_string());
                }
                let message = self.receive().await?;
                if message.get("method").is_some() && message.get("id").is_none() {
                    self.notifications.push_back(message);
                }
            }
        })
        .await
        .map_err(|_| format!("timed out waiting for notification `{method}`"))?
    }

    fn text_document(&self) -> Value {
        json!({"uri": URI})
    }
}

fn render_response(message: &Value) -> String {
    match message.get("error") {
        Some(error) => format!("error {} {}", error["code"], error["message"].as_str().unwrap_or("")),
        None => message["result"].to_string(),
    }
}

fn script_of(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix(';')?.trim_start().strip_prefix("lsp:"))
        .map(|command| command.trim().to_owned())
        .collect()
}

async fn step(client: &mut Client, source: &str, command: &str) -> Result<String, String> {
    let (verb, rest) = command.split_once(' ').unwrap_or((command, ""));
    match verb {
        "open" => {
            let item = json!({"uri": URI, "languageId": "clif", "version": client.version, "text": plain_text(source)});
            client.notify("textDocument/didOpen", json!({"textDocument": item})).await?;
            Ok("open".into())
        }
        "change" => {
            client.version += 1;
            let (range, text) = parse_edit(rest)?;
            let params = json!({
                "textDocument": {"uri": URI, "version": client.version},
                "contentChanges": [{"range": range, "text": text}],
            });
            client.notify("textDocument/didChange", params).await?;
            Ok(format!("change {rest}"))
        }
        "close" => {
            client.notify("textDocument/didClose", json!({"textDocument": client.text_document()})).await?;
            Ok("close".into())
        }
        "request" => {
            let (method, params) = rest.split_once(' ').unwrap_or((rest, "{}"));
            let mut params: Value = serde_json::from_str(params).map_err(|e| format!("bad params for `{method}`: {e}"))?;
            if method.starts_with("textDocument/") && params.get("textDocument").is_none() {
                params["textDocument"] = client.text_document();
            }
            Ok(format!("{method} -> {}", client.request(method, params).await?))
        }
        "tokens" => {
            let params = json!({"textDocument": client.text_document()});
            let response = client.request("textDocument/semanticTokens/full", params).await?;
            let data: Value = serde_json::from_str(&response).map_err(|_| response.clone())?;
            let numbers: Vec<u64> = data["data"].as_array().map_or(Vec::new(), |a| a.iter().filter_map(Value::as_u64).collect());
            let lines: Vec<Vec<u16>> = plain_text(source).lines().map(|l| l.encode_utf16().collect()).collect();
            let mut out = String::from("tokens");
            let (mut line, mut start) = (0u64, 0u64);
            for token in numbers.chunks(5) {
                line += token[0];
                start = if token[0] == 0 { start + token[1] } else { token[1] };
                let word = lines.get(line as usize).map_or(String::new(), |l| String::from_utf16_lossy(&l[start as usize..(start + token[2]) as usize]));
                let class = client.legend.get(token[3] as usize).map_or("?", String::as_str);
                out.push_str(&format!("\n{line}:{start} {class} {word}"));
            }
            Ok(out)
        }
        "complete" => {
            let (line, col) = rest.split_once(':').ok_or("expected `complete L:C`")?;
            let position = json!({"line": line.parse::<u32>().map_err(|e| e.to_string())?, "character": col.parse::<u32>().map_err(|e| e.to_string())?});
            let params = json!({"textDocument": client.text_document(), "position": position});
            let response = client.request("textDocument/completion", params).await?;
            let items: Value = serde_json::from_str(&response).map_err(|_| response.clone())?;
            let labels: Vec<&str> = items.as_array().map_or(Vec::new(), |a| a.iter().filter_map(|i| i["label"].as_str()).collect());
            let shown = if labels.is_empty() { "(none)".to_owned() } else { labels.join(" ") };
            Ok(format!("complete {rest} -> {shown}"))
        }
        "actions" => {
            let (range, _) = parse_edit(&format!("{rest} "))?;
            let params = json!({"textDocument": client.text_document(), "range": range, "context": {"diagnostics": []}});
            let response = client.request("textDocument/codeAction", params).await?;
            let actions: Value = serde_json::from_str(&response).map_err(|_| response.clone())?;
            let mut out = format!("actions {rest}");
            for action in actions.as_array().into_iter().flatten() {
                let edits = &action["edit"]["changes"][URI];
                let shown: Vec<String> = edits.as_array().into_iter().flatten().map(|e| {
                    let r = &e["range"];
                    format!("{}:{}-{}:{} {}", r["start"]["line"], r["start"]["character"], r["end"]["line"], r["end"]["character"], e["newText"])
                }).collect();
                let preferred = if action["isPreferred"] == true { " (preferred)" } else { "" };
                out.push_str(&format!("\n  {} [{}]{preferred} {}", action["title"].as_str().unwrap_or(""), action["kind"].as_str().unwrap_or(""), shown.join("; ")));
            }
            if actions.as_array().is_none_or(|a| a.is_empty()) {
                out.push_str("\n  (none)");
            }
            Ok(out)
        }
        "signature" => {
            let (line, col) = rest.split_once(':').ok_or("expected `signature L:C`")?;
            let position = json!({"line": line.parse::<u32>().map_err(|e| e.to_string())?, "character": col.parse::<u32>().map_err(|e| e.to_string())?});
            let params = json!({"textDocument": client.text_document(), "position": position});
            let response = client.request("textDocument/signatureHelp", params).await?;
            let help: Value = serde_json::from_str(&response).map_err(|_| response.clone())?;
            let Some(signature) = help["signatures"].get(0) else { return Ok(format!("signature {rest} -> (none)")) };
            let label = signature["label"].as_str().unwrap_or("");
            let chars: Vec<char> = label.chars().collect();
            let active = signature["activeParameter"].as_u64();
            let shown = signature["parameters"].as_array().into_iter().flatten().enumerate().map(|(i, p)| {
                let (start, end) = (p["label"][0].as_u64().unwrap_or(0) as usize, p["label"][1].as_u64().unwrap_or(0) as usize);
                let word: String = chars[start..end].iter().collect();
                if active == Some(i as u64) { format!("[{word}]") } else { word }
            }).collect::<Vec<_>>().join(" ");
            Ok(format!("signature {rest} -> {label} | {shown}"))
        }
        "folding" => {
            let response = client.request("textDocument/foldingRange", json!({"textDocument": client.text_document()})).await?;
            let ranges: Value = serde_json::from_str(&response).map_err(|_| response.clone())?;
            let mut out = String::from("folding");
            for r in ranges.as_array().into_iter().flatten() {
                out.push_str(&format!("\n{}-{} {}", r["startLine"], r["endLine"], r["kind"].as_str().unwrap_or("-")));
            }
            Ok(out)
        }
        "select" => {
            let (line, col) = rest.split_once(':').ok_or("expected `select L:C`")?;
            let position = json!({"line": line.parse::<u32>().map_err(|e| e.to_string())?, "character": col.parse::<u32>().map_err(|e| e.to_string())?});
            let params = json!({"textDocument": client.text_document(), "positions": [position]});
            let response = client.request("textDocument/selectionRange", params).await?;
            let mut node: Value = serde_json::from_str(&response).map_err(|_| response.clone())?;
            let mut node = node[0].take();
            let mut out = format!("select {rest} ->");
            while node.is_object() {
                let r = &node["range"];
                out.push_str(&format!(" {}:{}-{}:{}", r["start"]["line"], r["start"]["character"], r["end"]["line"], r["end"]["character"]));
                node = node["parent"].take();
            }
            Ok(out)
        }
        "calls" => {
            let (line, col) = rest.split_once(':').ok_or("expected `calls L:C`")?;
            let position = json!({"line": line.parse::<u32>().map_err(|e| e.to_string())?, "character": col.parse::<u32>().map_err(|e| e.to_string())?});
            let params = json!({"textDocument": client.text_document(), "position": position});
            let response = client.request("textDocument/prepareCallHierarchy", params).await?;
            let items: Value = serde_json::from_str(&response).map_err(|_| response.clone())?;
            let Some(item) = items.get(0).cloned() else { return Ok(format!("calls {rest} -> (none)")) };
            let span = |r: &Value| format!("{}:{}-{}:{}", r["start"]["line"], r["start"]["character"], r["end"]["line"], r["end"]["character"]);
            let mut out = format!("calls {rest} -> {} {}", item["name"].as_str().unwrap_or(""), span(&item["selectionRange"]));
            for (label, method, key) in [("incoming", "callHierarchy/incomingCalls", "from"), ("outgoing", "callHierarchy/outgoingCalls", "to")] {
                let response = client.request(method, json!({"item": item})).await?;
                let calls: Value = serde_json::from_str(&response).map_err(|_| response.clone())?;
                for call in calls.as_array().into_iter().flatten() {
                    let sites: Vec<String> = call["fromRanges"].as_array().into_iter().flatten().map(&span).collect();
                    out.push_str(&format!("\n  {label} {} [{}]", call[key]["name"].as_str().unwrap_or(""), sites.join(" ")));
                }
            }
            Ok(out)
        }
        "await" => Ok(format!("{rest} <- {}", client.await_notification(rest).await?)),
        _ => Err(format!("unknown lsp command `{command}`")),
    }
}

async fn session(source: &str) -> Result<String, String> {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let (server_read, server_write) = split(server_io);
    let (service, socket) = LspService::new(Backend::new);
    let server = tokio::spawn(Server::new(server_read, server_write, socket).serve(service));
    let (client_read, client_write) = split(client_io);
    let mut client = Client {
        reader: BufReader::new(client_read),
        writer: client_write,
        next_id: 0,
        version: 1,
        legend: Vec::new(),
        notifications: VecDeque::new(),
    };

    let mut out = String::new();
    let init = json!({"processId": null, "rootUri": null, "capabilities": {}});
    let initialized = client.request("initialize", init).await?;
    if let Ok(result) = serde_json::from_str::<Value>(&initialized) {
        let types = &result["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"];
        client.legend = types.as_array().map_or(Vec::new(), |a| a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect());
    }
    out.push_str(&format!("initialize -> {initialized}\n"));
    client.notify("initialized", json!({})).await?;
    for command in script_of(source) {
        out.push_str(&step(&mut client, source, &command).await?);
        out.push('\n');
    }
    out.push_str(&format!("shutdown -> {}\n", client.request("shutdown", Value::Null).await?));
    client.notify("exit", Value::Null).await?;
    tokio::time::timeout(TIMEOUT, server)
        .await
        .map_err(|_| "server did not stop after exit")?
        .map_err(|e| e.to_string())?;
    out.push_str("exit\n");
    Ok(out)
}

pub fn render(source: &str) -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    runtime.block_on(session(source))
}
