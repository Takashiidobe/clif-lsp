use std::collections::BTreeMap;

use tower_lsp_server::ls_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Position, Range, SymbolKind, Uri,
};

use crate::document::Document;
use crate::lexer::TokenKind;
use crate::syntax::{Function, Item, Span};
use crate::tables;

struct Call {
    caller: usize,
    callee: String,
    callee_span: Span,
    site: Span,
}

fn functions(document: &Document) -> impl Iterator<Item = (usize, &Function)> {
    document.file().items.iter().enumerate().filter_map(|(index, item)| match item {
        Item::Function(function) => Some((index, function)),
        Item::Directive(_) => None,
    })
}

fn named(document: &Document, function: &Function) -> Option<String> {
    function.name.map(|n| n.text(document.text())).filter(|n| n.starts_with('%')).map(str::to_owned)
}

fn calls(document: &Document) -> Vec<Call> {
    let text = document.text();
    let mut out = Vec::new();
    for (index, function) in functions(document) {
        let callee_of = |entity: &str| {
            function.decls.iter().find(|d| d.name.text(text) == entity).and_then(|d| d.rhs.iter().find(|t| t.kind == TokenKind::FuncName))
        };
        for inst in function.blocks.iter().flat_map(|b| &b.insts) {
            let is_call = inst.opcode.and_then(|o| tables::opcode(o.text(text))).is_some_and(|info| info.call);
            if !is_call {
                continue;
            }
            let target = inst.operands.iter().find_map(|t| match t.kind {
                TokenKind::FuncName => Some(*t),
                TokenKind::Entity => callee_of(t.text(text)).copied(),
                _ => None,
            });
            if let Some(token) = target {
                out.push(Call { caller: index, callee: token.text(text).to_owned(), callee_span: token.into(), site: inst.span });
            }
        }
    }
    out
}

fn range(document: &Document, span: Span) -> Range {
    Range::new(document.position(span.start), document.position(span.end))
}

fn item(document: &Document, uri: &Uri, name: &str, extent: Span, selection: Span) -> CallHierarchyItem {
    CallHierarchyItem {
        name: name.to_owned(),
        kind: SymbolKind::FUNCTION,
        tags: None,
        detail: None,
        uri: uri.clone(),
        range: range(document, extent),
        selection_range: range(document, selection),
        data: None,
    }
}

fn defined(document: &Document, uri: &Uri, name: &str) -> Option<CallHierarchyItem> {
    functions(document)
        .find(|(_, f)| named(document, f).as_deref() == Some(name))
        .map(|(_, f)| item(document, uri, name, f.span, f.name.unwrap_or(f.span)))
}

fn external(document: &Document, uri: &Uri, call: &Call) -> CallHierarchyItem {
    item(document, uri, &call.callee, call.callee_span, call.callee_span)
}

pub fn prepare(document: &Document, uri: &Uri, position: Position) -> Vec<CallHierarchyItem> {
    let offset = document.offset(position);
    let touches = |span: Span| span.start <= offset && offset <= span.end;
    let name = functions(document)
        .find_map(|(_, f)| f.name.filter(|n| touches(*n)).and_then(|_| named(document, f)))
        .or_else(|| calls(document).into_iter().find(|c| touches(c.callee_span)).map(|c| c.callee))
        .or_else(|| {
            let token = crate::navigation::find(&crate::navigation::resolution(document), offset)?;
            (token.kind == crate::resolve::NameKind::Function).then_some(token.name)
        });
    let Some(name) = name else { return Vec::new() };
    let found = defined(document, uri, &name).or_else(|| {
        let call = calls(document).into_iter().find(|c| c.callee == name)?;
        Some(external(document, uri, &call))
    });
    found.into_iter().collect()
}

pub fn incoming(document: &Document, uri: &Uri, target: &CallHierarchyItem) -> Vec<CallHierarchyIncomingCall> {
    let mut by_caller: BTreeMap<usize, Vec<Range>> = BTreeMap::new();
    for call in calls(document).into_iter().filter(|c| c.callee == target.name) {
        by_caller.entry(call.caller).or_default().push(range(document, call.site));
    }
    by_caller
        .into_iter()
        .filter_map(|(caller, from_ranges)| {
            let (_, function) = functions(document).find(|(i, _)| *i == caller)?;
            let name = named(document, function)?;
            Some(CallHierarchyIncomingCall { from: item(document, uri, &name, function.span, function.name.unwrap_or(function.span)), from_ranges })
        })
        .collect()
}

pub fn outgoing(document: &Document, uri: &Uri, source: &CallHierarchyItem) -> Vec<CallHierarchyOutgoingCall> {
    let Some((index, _)) = functions(document).find(|(_, f)| named(document, f).as_deref() == Some(source.name.as_str())) else {
        return Vec::new();
    };
    let mut by_callee: BTreeMap<String, (CallHierarchyItem, Vec<Range>)> = BTreeMap::new();
    for call in calls(document).into_iter().filter(|c| c.caller == index) {
        let entry = by_callee.entry(call.callee.clone()).or_insert_with(|| {
            (defined(document, uri, &call.callee).unwrap_or_else(|| external(document, uri, &call)), Vec::new())
        });
        entry.1.push(range(document, call.site));
    }
    by_callee.into_values().map(|(to, from_ranges)| CallHierarchyOutgoingCall { to, from_ranges }).collect()
}
