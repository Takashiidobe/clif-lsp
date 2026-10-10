use std::collections::HashMap;

use tower_lsp_server::ls_types::{CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic, Range, TextEdit, Uri, WorkspaceEdit};

use crate::diagnostics;
use crate::document::Document;
use crate::navigation;
use crate::resolve::NameKind;
use crate::syntax::{Function, Item, Span};
use crate::tables::{self, opcodes};

const SUGGESTIONS: usize = 3;

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != *cb)).min(above + 1).min(row[j] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

fn nearest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let limit = (name.chars().count() / 3).clamp(1, 3);
    let mut ranked: Vec<(usize, &str)> = candidates
        .map(|candidate| (edit_distance(name, candidate), candidate))
        .filter(|(distance, candidate)| *distance <= limit && *candidate != name)
        .collect();
    ranked.sort();
    ranked.dedup();
    ranked.into_iter().take(SUGGESTIONS).map(|(_, name)| name).collect()
}

fn quickfix(uri: &Uri, title: String, linked: &[Diagnostic], edit: TextEdit, preferred: bool) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(linked.to_vec()),
        edit: Some(WorkspaceEdit { changes: Some(HashMap::from([(uri.clone(), vec![edit])])), ..WorkspaceEdit::default() }),
        is_preferred: Some(preferred),
        ..CodeAction::default()
    })
}

fn overlaps(a: Range, b: Range) -> bool {
    a.start <= b.end && b.start <= a.end
}

fn unknown_opcodes(document: &Document) -> Vec<Span> {
    let source = document.text();
    document
        .file()
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(function) => Some(function),
            _ => None,
        })
        .flat_map(|function| function.blocks.iter().flat_map(|block| block.insts.iter()))
        .filter_map(|inst| inst.opcode)
        .filter(|span| tables::opcode(span.text(source)).is_none())
        .collect()
}

fn functions(document: &Document) -> impl Iterator<Item = (usize, &Function)> {
    document.file().items.iter().enumerate().filter_map(|(index, item)| match item {
        Item::Function(function) => Some((index, function)),
        Item::Directive(_) => None,
    })
}

fn line_start(document: &Document, offset: usize) -> usize {
    document.text()[..offset].rfind('\n').map_or(0, |n| n + 1)
}

fn stub_declaration(name: &str) -> Option<String> {
    let digits = name.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    match name.strip_suffix(digits)? {
        "ss" => Some(format!("{name} = explicit_slot 8")),
        "gv" => Some(format!("{name} = vmctx")),
        "sig" => Some(format!("{name} = () system_v")),
        _ => None,
    }
}

fn undefined_name_fixes(document: &Document, uri: &Uri, range: Range, published: &[Diagnostic], out: &mut Vec<CodeActionOrCommand>) {
    let resolution = navigation::resolution(document);
    for used in resolution.uses.iter().filter(|u| u.def.is_none() && u.kind != NameKind::Function) {
        let target = Range::new(document.position(used.span.start), document.position(used.span.end));
        if !overlaps(target, range) {
            continue;
        }
        let Some((scope, function)) = functions(document).find(|(_, f)| f.span.start <= used.span.start && used.span.start <= f.span.end) else {
            continue;
        };
        let linked: Vec<Diagnostic> = published.iter().filter(|d| d.range == target).cloned().collect();
        let defined = resolution.defs.iter().filter(|d| d.kind == used.kind && d.scope == Some(scope)).map(|d| d.name.as_str());
        for (rank, name) in nearest(&used.name, defined).into_iter().enumerate() {
            let title = format!("Change to `{name}`");
            out.push(quickfix(uri, title, &linked, TextEdit::new(target, name.to_owned()), rank == 0));
        }

        let stub = match used.kind {
            NameKind::Block if function.closed => {
                let at = document.position(line_start(document, function.span.end.saturating_sub(1)));
                Some((format!("Create block `{}`", used.name), at, format!("{}:\n    trap user1\n", used.name)))
            }
            NameKind::Entity => {
                let anchor = function.decls.first().map(|d| d.span.start).or(function.blocks.first().map(|b| b.span.start));
                let spacer = if function.decls.is_empty() { "\n" } else { "" };
                stub_declaration(&used.name).zip(anchor).map(|(text, anchor)| {
                    let at = document.position(line_start(document, anchor));
                    (format!("Declare `{}`", used.name), at, format!("    {text}\n{spacer}"))
                })
            }
            _ => None,
        };
        if let Some((title, at, text)) = stub {
            out.push(quickfix(uri, title, &linked, TextEdit::new(Range::new(at, at), text), false));
        }
    }
}

pub fn at(document: &Document, uri: &Uri, range: Range) -> Vec<CodeActionOrCommand> {
    let published = diagnostics::all(document);
    let mut actions = Vec::new();
    for span in unknown_opcodes(document) {
        let target = Range::new(document.position(span.start), document.position(span.end));
        if !overlaps(target, range) {
            continue;
        }
        let linked: Vec<Diagnostic> = published.iter().filter(|d| d.range == target).cloned().collect();
        let names = opcodes::OPCODES.iter().map(|info| info.name);
        for (rank, name) in nearest(span.text(document.text()), names).into_iter().enumerate() {
            let title = format!("Change to `{name}`");
            actions.push(quickfix(uri, title, &linked, TextEdit::new(target, name.to_owned()), rank == 0));
        }
    }
    undefined_name_fixes(document, uri, range, &published, &mut actions);
    actions
}
