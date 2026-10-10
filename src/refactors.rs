use std::collections::HashSet;

use tower_lsp_server::ls_types::{CodeActionKind, CodeActionOrCommand, Diagnostic, Range, TextEdit, Uri};

use crate::actions::{action_edits, functions, line_start, overlaps};
use crate::analysis::{self, FunctionAnalysis};
use crate::document::Document;
use crate::lexer::{Token, TokenKind};
use crate::navigation;
use crate::resolve::NameKind;
use crate::syntax::{Block, Function, Inst, Span};

const SAME_TYPE_OPERANDS: [&str; 14] =
    ["iadd", "isub", "imul", "udiv", "sdiv", "urem", "srem", "band", "bor", "bxor", "smin", "smax", "umin", "umax"];

fn range_of(document: &Document, span: Span) -> Range {
    Range::new(document.position(span.start), document.position(span.end))
}

fn squashed(text: &str) -> String {
    text.split_whitespace().collect()
}

fn alone_on_line(document: &Document, span: Span) -> bool {
    let text = document.text();
    let line = line_start(document, span.start);
    let rest = text[span.end..].find('\n').map_or(&text[span.end..], |n| &text[span.end..span.end + n]);
    text[line..span.start].trim().is_empty() && rest.trim().is_empty()
}

fn split_top_level(text: &str) -> Vec<&str> {
    let (mut parts, mut depth, mut start) = (Vec::new(), 0usize, 0);
    for (at, c) in text.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(text[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
}

fn collapse_brif(document: &Document, uri: &Uri, range: Range, out: &mut Vec<CodeActionOrCommand>) {
    let text = document.text();
    for (_, function) in functions(document) {
        for inst in function.blocks.iter().flat_map(|b| &b.insts) {
            let Some(opcode) = inst.opcode.filter(|o| o.text(text) == "brif") else { continue };
            if !overlaps(range_of(document, inst.span), range) {
                continue;
            }
            let [_, then, otherwise] = split_top_level(&text[opcode.end..inst.span.end])[..] else { continue };
            if squashed(then) != squashed(otherwise) || !then.starts_with("block") {
                continue;
            }
            let edit = TextEdit::new(range_of(document, inst.span), format!("jump {then}"));
            out.push(action_edits(uri, CodeActionKind::REFACTOR_REWRITE, "Collapse `brif` into `jump`".to_owned(), &[], vec![edit], false));
        }
    }
}

fn dominates(analysis: &FunctionAnalysis, ancestor: &str, block: &str) -> bool {
    let mut current = Some(block);
    while let Some(name) = current {
        if name == ancestor {
            return true;
        }
        current = analysis.blocks.iter().find(|b| b.name == name).and_then(|b| b.immediate_dominator.as_deref());
    }
    false
}

fn reuse_constants(document: &Document, uri: &Uri, range: Range, out: &mut Vec<CodeActionOrCommand>) {
    let text = document.text();
    let resolution = navigation::resolution(document);
    let analyses = analysis::analyze(document);
    for (scope, function) in functions(document) {
        let Some(analysis) = analyses.iter().find(|a| a.scope == scope) else { continue };
        let constants: Vec<(&Block, usize, &Inst, String)> = function
            .blocks
            .iter()
            .flat_map(|block| block.insts.iter().enumerate().map(move |(index, inst)| (block, index, inst)))
            .filter_map(|(block, index, inst)| {
                let opcode = inst.opcode.filter(|o| o.text(text) == "iconst")?;
                (inst.results.len() == 1).then(|| (block, index, inst, squashed(&text[opcode.start..inst.span.end])))
            })
            .collect();
        for &(block, index, inst, ref key) in &constants {
            if !overlaps(range_of(document, inst.span), range) || !alone_on_line(document, inst.span) {
                continue;
            }
            let earlier = constants.iter().find(|(b, i, _, k)| {
                k == key
                    && if b.name == block.name {
                        *i < index
                    } else {
                        dominates(analysis, b.name.text(text), block.name.text(text))
                    }
            });
            let Some((_, _, original, _)) = earlier else { continue };
            let kept = original.results[0].text(text);
            let Some(def) = resolution.defs.iter().position(|d| d.kind == NameKind::Value && d.span == Span::from(inst.results[0])) else {
                continue;
            };
            let mut edits: Vec<TextEdit> =
                resolution.uses.iter().filter(|u| u.def == Some(def)).map(|u| TextEdit::new(range_of(document, u.span), kept.to_owned())).collect();
            let line = line_start(document, inst.span.start);
            let end = text[inst.span.end..].find('\n').map_or(text.len(), |n| inst.span.end + n + 1);
            edits.push(TextEdit::new(range_of(document, Span { start: line, end }), String::new()));
            let title = format!("Reuse `{kept}` for duplicate constant");
            out.push(action_edits(uri, CodeActionKind::REFACTOR_REWRITE, title, &[], edits, false));
        }
    }
}

fn wrap_trailing(document: &Document, uri: &Uri, range: Range, out: &mut Vec<CodeActionOrCommand>) {
    let text = document.text();
    for (_, function) in functions(document) {
        for block in &function.blocks {
            for inst in block.insts.iter().skip(1) {
                let target = range_of(document, inst.span);
                if !(target.start <= range.start && range.start <= target.end) {
                    continue;
                }
                let line = line_start(document, inst.span.start);
                let indent = &text[line..inst.span.start];
                if !indent.trim().is_empty() {
                    continue;
                }
                let next = function
                    .blocks
                    .iter()
                    .filter_map(|b| b.name.text(text).strip_prefix("block")?.parse::<u64>().ok())
                    .max()
                    .map_or(0, |n| n + 1);
                let name = format!("block{next}");
                let at = document.position(line);
                let edit = TextEdit::new(Range::new(at, at), format!("{indent}jump {name}\n{name}:\n"));
                let title = format!("Move trailing instructions into new block `{name}`");
                out.push(action_edits(uri, CodeActionKind::REFACTOR_REWRITE, title, &[], vec![edit], false));
            }
        }
    }
}

fn block_param_type(document: &Document, function: &Function, inst: &Inst, used: Span) -> Option<String> {
    let text = document.text();
    let opcode = inst.opcode?.text(text);
    let values: Vec<Span> = inst.operands.iter().filter(|t| t.kind == TokenKind::Value).map(|&t| t.into()).collect();
    let position = values.iter().position(|v| *v == used)?;
    if opcode == "return" {
        let ty = function.signature.returns.get(position)?.text(text);
        return (!ty.contains(char::is_whitespace)).then(|| ty.to_owned());
    }

    let (mut target, mut depth, mut arg) = (None, 0usize, 0usize);
    for token in &inst.operands {
        match token.kind {
            TokenKind::Block if depth == 0 => (target, arg) = (Some(token.text(text)), 0),
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => depth = depth.saturating_sub(1),
            TokenKind::Comma if depth == 1 => arg += 1,
            TokenKind::Value if Span::from(*token) == used => {
                if let Some(name) = target.filter(|_| depth == 1) {
                    let block = function.blocks.iter().find(|b| b.name.text(text) == name)?;
                    return block.params.get(arg)?.ty.map(|ty| ty.text(text).to_owned());
                }
            }
            _ => {}
        }
    }

    if SAME_TYPE_OPERANDS.contains(&opcode) {
        let params = function.blocks.iter().flat_map(|b| &b.params);
        return values
            .iter()
            .filter(|v| **v != used)
            .find_map(|v| params.clone().find(|p| p.value.text(text) == v.text(text))?.ty.map(|ty| ty.text(text).to_owned()));
    }
    None
}

fn add_block_params(document: &Document, uri: &Uri, range: Range, published: &[Diagnostic], out: &mut Vec<CodeActionOrCommand>) {
    let text = document.text();
    let resolution = navigation::resolution(document);
    let mut offered = HashSet::new();
    for used in resolution.uses.iter().filter(|u| u.kind == NameKind::Value && u.def.is_none()) {
        let target = range_of(document, used.span);
        if !overlaps(target, range) {
            continue;
        }
        let Some((_, function)) = functions(document).find(|(_, f)| f.span.start <= used.span.start && used.span.start <= f.span.end) else {
            continue;
        };
        let Some((block_index, block)) = function.blocks.iter().enumerate().rev().find(|(_, b)| b.name.start <= used.span.start) else {
            continue;
        };
        let Some(inst) = block.insts.iter().find(|i| i.span.start <= used.span.start && used.span.end <= i.span.end) else { continue };
        if block_index == 0 || !offered.insert((block.name.start, used.name.clone())) {
            continue;
        }
        let Some(ty) = block_param_type(document, function, inst, used.span) else { continue };
        let (at, new_text) = match block.params.last() {
            Some(last) => (last.span.end, format!(", {}: {ty}", used.name)),
            None if text[block.name.end..].starts_with('(') => (block.name.end + 1, format!("{}: {ty}", used.name)),
            None => (block.name.end, format!("({}: {ty})", used.name)),
        };
        let edit = TextEdit::new(Range::new(document.position(at), document.position(at)), new_text);
        let linked: Vec<Diagnostic> = published.iter().filter(|d| d.range == target).cloned().collect();
        let title = format!("Add block parameter `{}: {ty}` to `{}`", used.name, block.name.text(text));
        out.push(action_edits(uri, CodeActionKind::QUICKFIX, title, &linked, vec![edit], false));
    }
}

struct BlockCall {
    name: Token,
    arguments: Vec<Span>,
    close: Token,
}

fn block_calls(text: &str, inst: &Inst, block: &str) -> Vec<BlockCall> {
    let tokens: Vec<Token> =
        inst.operands.iter().copied().filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Newline | TokenKind::Comment)).collect();
    let mut calls = Vec::new();
    for (at, name) in tokens.iter().enumerate() {
        if name.kind != TokenKind::Block || name.text(text) != block || tokens.get(at + 1).map(|t| t.kind) != Some(TokenKind::LParen) {
            continue;
        }
        let (mut depth, mut arguments, mut current) = (1usize, Vec::new(), None::<Span>);
        for token in &tokens[at + 2..] {
            match token.kind {
                TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RParen | TokenKind::RBracket => depth -= 1,
                TokenKind::Comma if depth == 1 => {
                    arguments.extend(current.take());
                    continue;
                }
                _ => {}
            }
            if depth == 0 {
                arguments.extend(current.take());
                calls.push(BlockCall { name: *name, arguments, close: *token });
                break;
            }
            let span = Span::from(*token);
            current = Some(current.map_or(span, |c| Span { start: c.start, end: span.end }));
        }
    }
    calls
}

fn removal(items: &[Span], index: usize) -> Span {
    if index + 1 < items.len() {
        Span { start: items[index].start, end: items[index + 1].start }
    } else {
        Span { start: items[index - 1].end, end: items[index].end }
    }
}

fn remove_unused_params(document: &Document, uri: &Uri, range: Range, out: &mut Vec<CodeActionOrCommand>) {
    let text = document.text();
    let resolution = navigation::resolution(document);
    for (_, function) in functions(document) {
        for block in function.blocks.iter().skip(1) {
            let spans: Vec<Span> = block.params.iter().map(|p| p.span).collect();
            for (index, param) in block.params.iter().enumerate() {
                if !overlaps(range_of(document, param.span), range) {
                    continue;
                }
                let Some(def) = resolution.defs.iter().position(|d| d.kind == NameKind::Value && d.span == Span::from(param.value)) else {
                    continue;
                };
                if resolution.uses.iter().any(|u| u.def == Some(def)) {
                    continue;
                }
                let header = if spans.len() == 1 {
                    let Some(close) = text[param.span.end..].find(')') else { continue };
                    Span { start: block.name.end, end: param.span.end + close + 1 }
                } else {
                    removal(&spans, index)
                };
                let mut edits = vec![TextEdit::new(range_of(document, header), String::new())];
                for inst in function.blocks.iter().flat_map(|b| &b.insts) {
                    for call in block_calls(text, inst, block.name.text(text)) {
                        if index >= call.arguments.len() {
                            continue;
                        }
                        let gone = if call.arguments.len() == 1 {
                            Span { start: call.name.end, end: call.close.end }
                        } else {
                            removal(&call.arguments, index)
                        };
                        edits.push(TextEdit::new(range_of(document, gone), String::new()));
                    }
                }
                let title = format!("Remove unused parameter `{}` from `{}`", param.value.text(text), block.name.text(text));
                out.push(action_edits(uri, CodeActionKind::REFACTOR_REWRITE, title, &[], edits, false));
            }
        }
    }
}

pub fn at(document: &Document, uri: &Uri, range: Range, published: &[Diagnostic], out: &mut Vec<CodeActionOrCommand>) {
    add_block_params(document, uri, range, published, out);
    remove_unused_params(document, uri, range, out);
    collapse_brif(document, uri, range, out);
    reuse_constants(document, uri, range, out);
    wrap_trailing(document, uri, range, out);
}
