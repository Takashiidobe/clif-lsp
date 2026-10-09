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
        "opcodes" => Ok(render_opcodes(&strip_directives(input.source))),
        "source" => Ok(strip_directives(input.source)),
        _ => Err(format!("unknown mode `{mode}`")),
    }
}

const DIRECTIVES: [&str; 7] = ["mode:", "check:", "sameln:", "nextln:", "unordered:", "not:", "regex:"];

fn strip_directives(source: &str) -> String {
    let is_directive = |line: &str| {
        let comment = line.trim_start().strip_prefix(';').map(str::trim_start);
        comment.is_some_and(|c| DIRECTIVES.iter().any(|d| c.starts_with(d)))
    };
    source.lines().filter(|l| !is_directive(l)).collect::<Vec<_>>().join("\n")
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
