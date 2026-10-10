use tower_lsp_server::ls_types::{Position, Range, TextEdit};

use crate::document::Document;
use crate::lexer::{Token, TokenKind as K, lex};
use crate::syntax::Item;

const INDENT: &str = "    ";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Region {
    TopLevel,
    Body,
    Untouched,
}

struct Line<'a> {
    content: &'a str,
    tokens: Vec<Token>,
}

impl Line<'_> {
    fn is_blank(&self) -> bool {
        self.tokens.is_empty()
    }

    fn is_comment_only(&self) -> bool {
        self.tokens.len() == 1 && self.tokens[0].kind == K::Comment
    }
}

pub fn edits(document: &Document, range: Option<Range>) -> Vec<TextEdit> {
    let text = document.text();
    let lines = split_lines(text);
    let regions = regions(document, lines.len());
    let wanted = |line: usize| range.is_none_or(|r| (r.start.line..=r.end.line).contains(&(line as u32)));
    let last_content = lines.iter().rposition(|l| !l.is_blank());
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let needs_final_newline = !text.is_empty() && !text.ends_with('\n');

    let mut out = Vec::new();
    let mut previous_blank = true;
    for (index, line) in lines.iter().enumerate() {
        let delete = line.is_blank() && (previous_blank || last_content.is_none_or(|last| index > last));
        previous_blank = line.is_blank();
        if !wanted(index) {
            continue;
        }
        let at = index as u32;
        if delete {
            out.push(TextEdit { range: Range::new(Position::new(at, 0), Position::new(at + 1, 0)), new_text: String::new() });
            continue;
        }
        let mut formatted = match regions[index] {
            _ if line.is_blank() => String::new(),
            Region::Untouched => line.content.to_owned(),
            Region::TopLevel => render(line, "", false),
            Region::Body => render(line, indent_of(&lines, &regions, index), true),
        };
        let add_newline = needs_final_newline && index + 1 == lines.len() && !line.is_blank();
        if add_newline {
            formatted.push_str(eol);
        }
        if formatted != line.content || add_newline {
            let end = line.content.encode_utf16().count() as u32;
            out.push(TextEdit { range: Range::new(Position::new(at, 0), Position::new(at, end)), new_text: formatted });
        }
    }
    out
}

pub fn apply(document: &mut Document, mut edits: Vec<TextEdit>) {
    edits.sort_by_key(|e| std::cmp::Reverse((e.range.start.line, e.range.start.character)));
    for edit in edits {
        document.apply(Some(edit.range), &edit.new_text);
    }
}

fn split_lines(text: &str) -> Vec<Line<'_>> {
    let body = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() {
        return Vec::new();
    }
    body.split('\n')
        .map(|raw| {
            let content = raw.strip_suffix('\r').unwrap_or(raw);
            let tokens = lex(content).into_iter().filter(|t| t.kind != K::Whitespace).collect();
            Line { content, tokens }
        })
        .collect()
}

fn regions(document: &Document, line_count: usize) -> Vec<Region> {
    let mut regions = vec![Region::TopLevel; line_count];
    for item in &document.file().items {
        let Item::Function(function) = item else { continue };
        let first = document.position(function.span.start).line as usize;
        let last = (document.position(function.span.end).line as usize).min(line_count.saturating_sub(1));
        let region = if function.closed { Region::Body } else { Region::Untouched };
        for slot in regions.iter_mut().take(last + 1).skip(first) {
            *slot = region;
        }
    }
    regions
}

fn indent_of(lines: &[Line], regions: &[Region], index: usize) -> &'static str {
    let line = &lines[index];
    let flush_left = if line.is_comment_only() {
        lines[index + 1..]
            .iter()
            .zip(&regions[index + 1..])
            .take_while(|(_, region)| **region == Region::Body)
            .map(|(l, _)| l)
            .find(|l| !l.is_comment_only() && !l.is_blank())
            .is_some_and(|l| l.tokens[0].kind == K::Block)
    } else {
        matches!(line.tokens[0].kind, K::Block | K::RBrace) || is_function_keyword(line)
    };
    if flush_left { "" } else { INDENT }
}

fn is_function_keyword(line: &Line) -> bool {
    let first = line.tokens[0];
    first.kind == K::Ident && &line.content[first.start..first.end] == "function"
}

fn render(line: &Line, indent: &str, punctuation: bool) -> String {
    let mut out = String::from(indent);
    let mut previous: Option<Token> = None;
    let mut before_previous: Option<Token> = None;
    for &token in &line.tokens {
        if let Some(prev) = previous {
            let gap = token.start > prev.end;
            let space = if punctuation { spaced(before_previous, prev, token, gap) } else { gap || token.kind == K::Comment };
            if space {
                out.push(' ');
            }
        }
        let text = &line.content[token.start..token.end];
        out.push_str(if token.kind == K::Comment { text.trim_end() } else { text });
        before_previous = previous;
        previous = Some(token);
    }
    out
}

fn spaced(before: Option<Token>, prev: Token, token: Token, gap: bool) -> bool {
    match (prev.kind, token.kind) {
        (_, K::Comma | K::RParen | K::RBracket) => false,
        (K::LParen | K::LBracket, _) => false,
        (K::Comma, _) => true,
        (K::Eq | K::Arrow, _) | (_, K::Eq | K::Arrow) => true,
        (_, K::Colon) => false,
        (K::Colon, _) => before.is_some_and(|b| b.kind == K::Value) || gap,
        (_, K::Comment | K::LBrace) => true,
        _ => gap,
    }
}
