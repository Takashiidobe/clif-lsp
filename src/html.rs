pub const ROW_HEIGHT: usize = 26;

const BASE_CSS: &str = r#"
:root {
  --row: 26px;
  --bg: #fbfbfa; --fg: #1f2328; --muted: #6a737d; --line: #8b949e; --border: #d0d7de; --head: #eef0f2; --hl: #1f6feb;
  --removed: #f4b6b6; --changed: #f7e7a0; --new: #b6e3b6;
  --spill: #f4b6b6; --reload: #b6e3b6; --move: #f7e7a0; --reg: #dbe8f7; --stack: #e6d5f0;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #14171a; --fg: #e6edf3; --muted: #8b949e; --line: #6e7681; --border: #30363d; --head: #1f242a; --hl: #58a6ff;
    --removed: #6e2b2b; --changed: #6b5a1a; --new: #24502b;
    --spill: #6e2b2b; --reload: #24502b; --move: #6b5a1a; --reg: #1f3a5f; --stack: #3d2a52;
  }
}
body { margin: 0; padding: 16px; background: var(--bg); color: var(--fg); font: 14px/1.4 system-ui, sans-serif; }
h1 { font-size: 16px; margin: 0 0 4px; }
.legend { color: var(--muted); margin: 0 0 12px; display: flex; flex-wrap: wrap; gap: 4px 16px; }
.swatch { display: inline-block; width: 12px; height: 12px; border: 1px solid var(--border); vertical-align: -1px; margin-right: 4px; }
.mono { font: 13px/var(--row) ui-monospace, SFMono-Regular, Menlo, monospace; }
.hl { outline: 2px solid var(--hl); outline-offset: -2px; }
"#;

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn page(title: &str, legend: &[(&str, &str)], css: &str, body: &str, script: &str) -> String {
    let title = escape(title);
    let mut out = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{title}</title>\n<style>{BASE_CSS}{css}</style>\n</head>\n<body>\n<h1>{title}</h1>\n<p class=\"legend\">"
    );
    for (color, label) in legend {
        out.push_str(&format!("<span><i class=\"swatch\" style=\"background:var(--{color})\"></i>{}</span>", escape(label)));
    }
    out.push_str("</p>\n");
    out.push_str(body);
    if !script.is_empty() {
        out.push_str(&format!("<script>\n{script}</script>\n"));
    }
    out.push_str("</body>\n</html>\n");
    out
}
