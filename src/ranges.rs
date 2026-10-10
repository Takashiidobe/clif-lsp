use tower_lsp_server::ls_types::{FoldingRange, FoldingRangeKind, Position, Range, SelectionRange};

use crate::document::Document;
use crate::syntax::{Block, Function, Item, Span};

fn functions(document: &Document) -> impl Iterator<Item = &Function> {
    document.file().items.iter().filter_map(|item| match item {
        Item::Function(function) => Some(function),
        Item::Directive(_) => None,
    })
}

fn block_span(block: &Block) -> Span {
    Span { start: block.span.start, end: block.insts.last().map_or(block.span.end, |inst| inst.span.end) }
}

fn fold(document: &Document, span: Span, trim_last: bool, kind: Option<FoldingRangeKind>) -> Option<FoldingRange> {
    let start_line = document.position(span.start).line;
    let end_line = document.position(span.end).line.saturating_sub(u32::from(trim_last));
    (end_line > start_line).then_some(FoldingRange {
        start_line,
        start_character: None,
        end_line,
        end_character: None,
        kind,
        collapsed_text: None,
    })
}

fn comment_runs(document: &Document) -> Vec<FoldingRange> {
    let text = document.text();
    let lines: Vec<u32> = document
        .file()
        .comments
        .iter()
        .filter(|c| {
            let line_start = text[..c.start].rfind('\n').map_or(0, |n| n + 1);
            text[line_start..c.start].trim().is_empty()
        })
        .map(|c| document.position(c.start).line)
        .collect();
    let mut out = Vec::new();
    let mut run: Option<(u32, u32)> = None;
    for line in lines {
        run = match run {
            Some((start, end)) if line == end + 1 => Some((start, line)),
            Some((start, end)) => {
                out.extend(comment_fold(start, end));
                Some((line, line))
            }
            None => Some((line, line)),
        };
    }
    if let Some((start, end)) = run {
        out.extend(comment_fold(start, end));
    }
    out
}

fn comment_fold(start_line: u32, end_line: u32) -> Option<FoldingRange> {
    (end_line > start_line).then_some(FoldingRange {
        start_line,
        start_character: None,
        end_line,
        end_character: None,
        kind: Some(FoldingRangeKind::Comment),
        collapsed_text: None,
    })
}

pub fn folding(document: &Document) -> Vec<FoldingRange> {
    let mut out = Vec::new();
    for function in functions(document) {
        out.extend(fold(document, function.span, function.closed, Some(FoldingRangeKind::Region)));
        for block in &function.blocks {
            out.extend(fold(document, block_span(block), false, Some(FoldingRangeKind::Region)));
        }
    }
    out.extend(comment_runs(document));
    out.sort_by_key(|r| (r.start_line, r.end_line));
    out
}

fn word_around(text: &str, offset: usize) -> Option<Span> {
    let part = |c: char| c.is_alphanumeric() || matches!(c, '_' | '%' | '$' | '.' | '@');
    let start = text[..offset].char_indices().rev().take_while(|(_, c)| part(*c)).last().map_or(offset, |(i, _)| i);
    let end = offset + text[offset..].char_indices().take_while(|(_, c)| part(*c)).last().map_or(0, |(i, c)| i + c.len_utf8());
    (start < end).then_some(Span { start, end })
}

pub fn selection(document: &Document, position: Position) -> SelectionRange {
    let offset = document.offset(position);
    let mut chain: Vec<Span> = Vec::new();
    chain.extend(word_around(document.text(), offset));
    for function in functions(document).filter(|f| f.span.start <= offset && offset <= f.span.end) {
        let line = function.decls.iter().map(|d| d.span).chain(function.blocks.iter().flat_map(|b| b.insts.iter().map(|i| i.span)));
        chain.extend(line.filter(|s| s.start <= offset && offset <= s.end));
        chain.extend(function.blocks.iter().map(block_span).filter(|s| s.start <= offset && offset <= s.end));
        chain.push(function.span);
    }
    chain.push(Span { start: 0, end: document.text().len() });

    let mut nested: Vec<Span> = Vec::new();
    for span in chain {
        if nested.last().is_none_or(|inner| span.start <= inner.start && inner.end <= span.end && span != *inner) {
            nested.push(span);
        }
    }
    let range = |s: Span| Range::new(document.position(s.start), document.position(s.end));
    nested.into_iter().rev().fold(None, |parent, span| Some(SelectionRange { range: range(span), parent: parent.map(Box::new) })).expect("chain is never empty")
}
