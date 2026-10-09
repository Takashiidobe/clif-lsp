use tower_lsp_server::ls_types::{Position, Range};

use crate::document::Document;
use crate::lexer::{self, Token, TokenKind};
use crate::parser;
use crate::resolve;
use crate::syntax::{Item, Span};

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
        "resolve" => Ok(render_resolution(input.source)),
        "document" => render_document(input),
        "syntax-tree" => Ok(render_syntax_tree(input.source)),
        "names" => Ok(render_names(&strip_directives(input.source))),
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

struct Positions {
    line_starts: Vec<usize>,
    header: usize,
}

impl Positions {
    fn new(source: &str) -> Positions {
        let mut line_starts = vec![0];
        line_starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));
        let header = source.lines().take_while(|l| is_directive(l)).count();
        Positions { line_starts, header }
    }

    fn at(&self, byte: usize) -> String {
        let line = self.line_starts.partition_point(|&s| s <= byte) - 1;
        let rel = line as isize - self.header as isize;
        format!("{rel}:{}", byte - self.line_starts[line])
    }

    fn range(&self, span: Span) -> String {
        format!("{}..{}", self.at(span.start), self.at(span.end))
    }
}

fn join_tokens(source: &str, tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.text(source)).collect::<Vec<_>>().join(" ")
}

fn join_spans(source: &str, spans: &[Span]) -> String {
    spans.iter().map(|s| s.text(source)).collect::<Vec<_>>().join(", ")
}

fn render_syntax_tree(source: &str) -> String {
    let file = parser::parse(source);
    let pos = Positions::new(source);
    let mut out = String::new();
    for item in &file.items {
        match item {
            Item::Directive(d) => {
                out.push_str(&format!(
                    "directive @{} {} {}\n",
                    pos.at(d.span.start),
                    d.keyword.text(source),
                    join_tokens(source, &d.args)
                ));
            }
            Item::Function(f) => {
                let name = f.name.map_or("<missing>", |n| n.text(source));
                let state = if f.closed { "closed" } else { "unclosed" };
                out.push_str(&format!("function {name} @{} {state}\n", pos.at(f.span.start)));
                out.push_str(&format!(
                    "  signature ({}) -> ({})\n",
                    join_spans(source, &f.signature.params),
                    join_spans(source, &f.signature.returns)
                ));
                if let Some(cc) = f.signature.call_conv {
                    out.push_str(&format!("  call_conv {}\n", cc.text(source)));
                }
                for decl in &f.decls {
                    out.push_str(&format!(
                        "  decl {} @{} = {}\n",
                        decl.name.text(source),
                        pos.at(decl.span.start),
                        join_tokens(source, &decl.rhs)
                    ));
                }
                for block in &f.blocks {
                    out.push_str(&format!("  block {} @{}\n", block.name.text(source), pos.at(block.span.start)));
                    for param in &block.params {
                        let ty = param.ty.map_or("<missing>", |t| t.text(source));
                        out.push_str(&format!("    param {} : {ty}\n", param.value.text(source)));
                    }
                    for inst in &block.insts {
                        let opcode = inst.opcode.map_or("<missing>", |o| o.text(source));
                        let suffix: String = inst.type_suffix.iter().map(|s| format!(".{}", s.text(source))).collect();
                        let results = join_tokens(source, &inst.results);
                        let mut parts = vec![format!("inst @{}", pos.at(inst.span.start))];
                        if !results.is_empty() {
                            parts.push(format!("{results} ="));
                        }
                        parts.push(format!("{opcode}{suffix}"));
                        parts.push(join_tokens(source, &inst.operands));
                        out.push_str(&format!("    {}\n", parts.join(" ").trim_end()));
                    }
                }
            }
        }
    }
    out.push_str("errors:\n");
    for error in &file.errors {
        out.push_str(&format!("  {} {}\n", pos.range(error.span), error.message));
    }
    out.push_str("comments:\n");
    let (directives, plain): (Vec<&Span>, Vec<&Span>) =
        file.comments.iter().partition(|c| is_directive(c.text(source)));
    out.push_str(&format!("  {} directive comments\n", directives.len()));
    for comment in plain {
        out.push_str(&format!("  @{} {:?}\n", pos.at(comment.start), comment.text(source)));
    }
    out
}

fn render_resolution(source: &str) -> String {
    let file = parser::parse(source);
    let resolution = resolve::resolve(&file, source);
    let pos = Positions::new(source);

    let mut lines: Vec<(usize, String)> = Vec::new();
    for def in &resolution.defs {
        lines.push((def.span.start, format!("@{} def {} {}", pos.at(def.span.start), def.kind.label(), def.name)));
    }
    for usage in &resolution.uses {
        let target = match usage.def {
            Some(id) => format!("@{}", pos.at(resolution.defs[id].span.start)),
            None if usage.kind == resolve::NameKind::Function => "<external>".to_owned(),
            None => "<undefined>".to_owned(),
        };
        lines.push((
            usage.span.start,
            format!("@{} use {} {} -> {target}", pos.at(usage.span.start), usage.kind.label(), usage.name),
        ));
    }
    lines.sort_by_key(|(start, _)| *start);

    let mut out = String::new();
    for (_, line) in lines {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("problems:\n");
    for problem in &resolution.problems {
        out.push_str(&format!("  {} {}\n", pos.range(problem.span), problem.message));
    }
    out
}

fn render_names(source: &str) -> String {
    use crate::tables::{codes, lookup, types};
    let categories = [
        ("type", types::TYPES),
        ("intcc", codes::INT_CC),
        ("floatcc", codes::FLOAT_CC),
        ("trap", codes::TRAP_CODES),
        ("memflag", codes::MEM_FLAGS),
        ("callconv", codes::CALL_CONVS),
    ];
    let mut out = String::new();
    for word in source.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let hits: Vec<String> = categories
            .iter()
            .filter_map(|(label, table)| lookup(table, word).map(|e| format!("{label} {:?}", e.doc)))
            .collect();
        if !hits.is_empty() {
            out.push_str(&format!("{word}: {}\n", hits.join("; ")));
        }
    }
    out
}

fn render_document(input: &Input) -> Result<String, String> {
    let mut out = String::new();
    let raw = Document::new(input.source);
    let pos = Positions::new(input.source);
    for query in input.queries.iter().filter(|q| q.kind == "at") {
        let offset = pos.line_starts[query.line] + query.col;
        let lsp = raw.position(offset);
        let back = raw.offset(lsp) - pos.line_starts[query.line];
        out.push_str(&format!(
            "at {}:{} -> utf16 {} -> byte {back}\n",
            query.line as isize - pos.header as isize,
            query.col,
            lsp.character
        ));
    }

    let edits: Vec<&Query> = input.queries.iter().filter(|q| q.kind == "edit").collect();
    if !edits.is_empty() {
        let plain: Vec<&str> = input.source.lines().filter(|l| !is_directive(l) && !is_caret(l)).collect();
        let mut document = Document::new(&plain.join("\n"));
        for edit in edits {
            let (range, text) = parse_edit(&edit.args)?;
            document.apply(Some(range), &text);
            out.push_str(&format!("edit {}\n", edit.args));
        }
        out.push_str(&format!("text:\n{}\nerrors: {}\n", document.text(), document.file().errors.len()));
    }
    Ok(out)
}

fn is_caret(line: &str) -> bool {
    line.trim_start().strip_prefix(';').is_some_and(|rest| rest.trim_start().starts_with('^'))
}

fn parse_edit(args: &str) -> Result<(Range, String), String> {
    let bad = || format!("bad edit `{args}`, expected `L:C-L:C text`");
    let (range, text) = args.split_once(' ').unwrap_or((args, ""));
    let (start, end) = range.split_once('-').ok_or_else(bad)?;
    let position = |s: &str| -> Result<Position, String> {
        let (line, col) = s.split_once(':').ok_or_else(bad)?;
        Ok(Position::new(line.parse().map_err(|_| bad())?, col.parse().map_err(|_| bad())?))
    };
    Ok((Range::new(position(start)?, position(end)?), text.replace("\\n", "\n")))
}
