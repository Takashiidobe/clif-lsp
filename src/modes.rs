use crate::lexer::{self, TokenKind};

pub struct Query {
    pub line: usize,
    pub col: usize,
    pub kind: String,
    pub args: String,
}

pub struct Input<'a> {
    pub source: &'a str,
    pub queries: &'a [Query],
}

pub fn render(mode: &str, input: &Input) -> Result<String, String> {
    match mode {
        "tokens" => render_tokens(input.source),
        "opcodes" => Ok(render_opcodes(&strip_directives(input.source))),
        "source" => Ok(strip_directives(input.source)),
        _ => Err(format!("unknown mode `{mode}`")),
    }
}

const DIRECTIVES: [&str; 7] = ["mode:", "check:", "sameln:", "nextln:", "unordered:", "not:", "regex:"];

fn is_directive(comment: &str) -> bool {
    let body = comment.trim_start().strip_prefix(';').map(str::trim_start);
    body.is_some_and(|c| DIRECTIVES.iter().any(|d| c.starts_with(d)))
}

fn strip_directives(source: &str) -> String {
    source.lines().filter(|l| !is_directive(l)).collect::<Vec<_>>().join("\n")
}

fn render_tokens(source: &str) -> Result<String, String> {
    let tokens = lexer::lex(source);
    let mut expected_start = 0;
    for token in &tokens {
        if token.start != expected_start {
            return Err(format!("lexer gap or overlap at byte {}", token.start));
        }
        expected_start = token.end;
    }
    if expected_start != source.len() {
        return Err(format!("lexer stopped at byte {expected_start} of {}", source.len()));
    }

    let header = source.lines().take_while(|l| is_directive(l)).count();
    let mut out = String::new();
    let (mut line, mut line_start) = (0usize, 0usize);
    for token in &tokens {
        let text = token.text(source);
        match token.kind {
            TokenKind::Whitespace => continue,
            TokenKind::Newline => {
                line += 1;
                line_start = token.end;
                continue;
            }
            TokenKind::Comment if is_directive(text) => {
                out.push_str("Comment \"<directive>\"\n");
            }
            kind => {
                let rel = line - header;
                out.push_str(&format!("{rel}:{} {kind:?} {text:?}\n", token.start - line_start));
            }
        }
    }
    Ok(out)
}

fn render_opcodes(source: &str) -> String {
    let mut out = String::new();
    for word in source.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if let Some(op) = crate::tables::opcode(word) {
            out.push_str(&format!("{}: {} [{}]\n", op.name, op.signature, op.format));
        }
    }
    out
}
