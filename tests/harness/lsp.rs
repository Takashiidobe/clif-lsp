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
        notifications: VecDeque::new(),
    };

    let mut out = String::new();
    let init = json!({"processId": null, "rootUri": null, "capabilities": {}});
    out.push_str(&format!("initialize -> {}\n", client.request("initialize", init).await?));
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
