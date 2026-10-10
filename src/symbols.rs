use tower_lsp_server::ls_types::{DocumentSymbol, Range, SymbolKind};

use crate::document::Document;
use crate::syntax::{Item, Span};

fn range(document: &Document, span: Span) -> Range {
    Range::new(document.position(span.start), document.position(span.end))
}

fn decl_kind(name: &str) -> SymbolKind {
    let prefix = name.trim_end_matches(|c: char| c.is_ascii_digit());
    match prefix {
        "sig" => SymbolKind::INTERFACE,
        "fn" => SymbolKind::METHOD,
        "ss" | "dss" => SymbolKind::VARIABLE,
        "gv" => SymbolKind::FIELD,
        "jt" => SymbolKind::ENUM,
        "dt" => SymbolKind::CLASS,
        _ => SymbolKind::CONSTANT,
    }
}

#[allow(deprecated)]
fn symbol(
    document: &Document,
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    span: Span,
    selection: Span,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range: range(document, span),
        selection_range: range(document, selection),
        children: Some(children),
    }
}

pub fn document(document: &Document) -> Vec<DocumentSymbol> {
    let text = document.text();
    let mut out = Vec::new();
    for item in &document.file().items {
        let Item::Function(function) = item else {
            continue;
        };
        let name_span = function.name.unwrap_or(function.span);
        let name = function
            .name
            .map_or_else(|| "function".to_owned(), |n| n.text(text).to_owned());

        let mut children = Vec::new();
        for decl in &function.decls {
            let decl_name = decl.name.text(text).to_owned();
            let detail = decl
                .rhs
                .first()
                .zip(decl.rhs.last())
                .map(|(a, b)| text[a.start..b.end].to_owned());
            let kind = decl_kind(&decl_name);
            children.push(symbol(
                document,
                decl_name,
                detail,
                kind,
                decl.span,
                decl.name.into(),
                Vec::new(),
            ));
        }
        for block in &function.blocks {
            let block_name = block.name.text(text).to_owned();
            let params = block
                .params
                .iter()
                .map(|p| p.span.text(text))
                .collect::<Vec<_>>()
                .join(", ");
            let detail = Some(format!("({params})")).filter(|_| !block.params.is_empty());
            let extent = Span {
                start: block.span.start,
                end: block.insts.last().map_or(block.span.end, |i| i.span.end),
            };
            children.push(symbol(
                document,
                block_name,
                detail,
                SymbolKind::NAMESPACE,
                extent,
                block.name.into(),
                Vec::new(),
            ));
        }

        let detail = text[function.span.start..function.span.end]
            .lines()
            .next()
            .map(|l| l.trim().to_owned());
        let detail = detail.map(|l| l.trim_end_matches('{').trim_end().to_owned());
        out.push(symbol(
            document,
            name,
            detail,
            SymbolKind::FUNCTION,
            function.span,
            name_span,
            children,
        ));
    }
    out
}
