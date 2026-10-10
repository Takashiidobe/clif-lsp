use tower_lsp_server::ls_types::{Position, Range, TextEdit};

use crate::document::Document;
use crate::lexer::{self, TokenKind};
use crate::resolve::{self, NameKind, Resolution};
use crate::syntax::Span;

pub struct Target {
    pub kind: NameKind,
    pub name: String,
    pub span: Span,
    pub def: Option<usize>,
}

fn range(document: &Document, span: Span) -> Range {
    Range::new(document.position(span.start), document.position(span.end))
}

pub fn resolution(document: &Document) -> Resolution {
    resolve::resolve(document.file(), document.text())
}

pub fn find(resolution: &Resolution, offset: usize) -> Option<Target> {
    let hit = |span: Span| span.start <= offset && offset < span.end;
    let touch = |span: Span| span.start <= offset && offset <= span.end;
    let defs = || {
        resolution.defs.iter().enumerate().map(|(id, d)| (d.span, Target { kind: d.kind, name: d.name.clone(), span: d.span, def: Some(id) }))
    };
    let uses = || resolution.uses.iter().map(|u| (u.span, Target { kind: u.kind, name: u.name.clone(), span: u.span, def: u.def }));
    let all = || defs().chain(uses());
    all().find(|(span, _)| hit(*span)).or_else(|| all().find(|(span, _)| touch(*span))).map(|(_, target)| target)
}

pub fn definition(document: &Document, position: Position) -> Option<Range> {
    let resolution = resolution(document);
    let target = find(&resolution, document.offset(position))?;
    target.def.map(|id| range(document, resolution.defs[id].span))
}

fn occurrences(resolution: &Resolution, target: &Target) -> Vec<Span> {
    match target.def {
        Some(id) => {
            let mut spans = vec![resolution.defs[id].span];
            spans.extend(resolution.uses.iter().filter(|u| u.def == Some(id)).map(|u| u.span));
            spans
        }
        None if target.kind == NameKind::Function => resolution
            .uses
            .iter()
            .filter(|u| u.kind == NameKind::Function && u.def.is_none() && u.name == target.name)
            .map(|u| u.span)
            .collect(),
        None => vec![target.span],
    }
}

pub fn references(document: &Document, position: Position, include_declaration: bool) -> Vec<Range> {
    let resolution = resolution(document);
    let Some(target) = find(&resolution, document.offset(position)) else { return Vec::new() };
    let declaration = target.def.map(|id| resolution.defs[id].span);
    occurrences(&resolution, &target)
        .into_iter()
        .filter(|span| include_declaration || Some(*span) != declaration)
        .map(|span| range(document, span))
        .collect()
}

pub fn prepare_rename(document: &Document, position: Position) -> Option<(Range, String)> {
    let resolution = resolution(document);
    let target = find(&resolution, document.offset(position))?;
    let renamable = target.def.is_some() || target.kind == NameKind::Function;
    renamable.then(|| (range(document, target.span), target.name))
}

fn entity_prefix(name: &str) -> &str {
    name.trim_end_matches(|c: char| c.is_ascii_digit())
}

pub fn rename(document: &Document, position: Position, new_name: &str) -> Result<Vec<TextEdit>, String> {
    let resolution = resolution(document);
    let target = find(&resolution, document.offset(position)).ok_or("nothing to rename here")?;
    if target.def.is_none() && target.kind != NameKind::Function {
        return Err(format!("`{}` has no definition to rename", target.name));
    }

    let tokens = lexer::lex(new_name);
    let expected = match target.kind {
        NameKind::Value => TokenKind::Value,
        NameKind::Block => TokenKind::Block,
        NameKind::Entity => TokenKind::Entity,
        NameKind::Function => TokenKind::FuncName,
    };
    if tokens.len() != 1 || tokens[0].kind != expected || tokens[0].end != new_name.len() {
        return Err(format!("`{new_name}` is not a valid {} name", target.kind.label()));
    }
    if target.kind == NameKind::Entity && entity_prefix(new_name) != entity_prefix(&target.name) {
        return Err(format!("`{new_name}` must keep the `{}` prefix", entity_prefix(&target.name)));
    }

    let scope = target.def.and_then(|id| resolution.defs[id].scope);
    let clash = resolution.defs.iter().any(|d| d.kind == target.kind && d.scope == scope && d.name == new_name);
    if clash && new_name != target.name {
        return Err(format!("`{new_name}` is already defined"));
    }

    Ok(occurrences(&resolution, &target)
        .into_iter()
        .map(|span| TextEdit { range: range(document, span), new_text: new_name.to_owned() })
        .collect())
}
