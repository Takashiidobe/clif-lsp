use std::collections::HashMap;

use tower_lsp_server::ls_types::{CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic, Range, TextEdit, Uri, WorkspaceEdit};

use crate::diagnostics;
use crate::document::Document;
use crate::syntax::{Item, Span};
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

fn nearest_opcodes(name: &str) -> Vec<&'static str> {
    let limit = (name.chars().count() / 3).clamp(1, 3);
    let mut ranked: Vec<(usize, &'static str)> = opcodes::OPCODES
        .iter()
        .map(|info| (edit_distance(name, info.name), info.name))
        .filter(|(distance, _)| *distance <= limit)
        .collect();
    ranked.sort();
    ranked.into_iter().take(SUGGESTIONS).map(|(_, name)| name).collect()
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

pub fn at(document: &Document, uri: &Uri, range: Range) -> Vec<CodeActionOrCommand> {
    let published = diagnostics::all(document);
    let mut actions = Vec::new();
    for span in unknown_opcodes(document) {
        let target = Range::new(document.position(span.start), document.position(span.end));
        if !overlaps(target, range) {
            continue;
        }
        let linked: Vec<Diagnostic> = published.iter().filter(|d| d.range == target).cloned().collect();
        for (rank, name) in nearest_opcodes(span.text(document.text())).into_iter().enumerate() {
            let edit = TextEdit::new(target, name.to_owned());
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Change to `{name}`"),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(linked.clone()),
                edit: Some(WorkspaceEdit { changes: Some(HashMap::from([(uri.clone(), vec![edit])])), ..WorkspaceEdit::default() }),
                is_preferred: Some(rank == 0),
                ..CodeAction::default()
            }));
        }
    }
    actions
}
