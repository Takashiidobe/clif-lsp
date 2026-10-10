use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use crate::document::Document;
use crate::inference;
use crate::navigation;
use crate::resolve::{NameKind, Resolution};
use crate::syntax::{Function, Item, Span};
use crate::tables::{self, NameInfo, codes, types};

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

fn markdown(document: &Document, span: Span, body: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: body }),
        range: Some(Range::new(document.position(span.start), document.position(span.end))),
    }
}

fn named(info: &NameInfo) -> String {
    format!("`{}`\n\n{}", info.name, info.doc)
}

fn opcode_flags(info: &tables::OpcodeInfo) -> String {
    let flags = [
        ("terminator", info.terminator),
        ("branch", info.branch),
        ("call", info.call),
        ("return", info.ret),
        ("load", info.load),
        ("store", info.store),
        ("trap", info.trap),
        ("side effects", info.side_effects),
    ];
    flags.iter().filter(|(_, on)| *on).map(|(name, _)| *name).collect::<Vec<_>>().join(", ")
}

fn opcode_hover(info: &tables::OpcodeInfo) -> String {
    let mut out = format!("```clif\n{}\n```\n\n{}", info.signature, info.doc);
    let flags = opcode_flags(info);
    if !flags.is_empty() {
        out.push_str(&format!("\n\n_{flags}_"));
    }
    out
}

fn first_line<'a>(text: &'a str, span: Span) -> &'a str {
    text[span.start..span.end].lines().next().unwrap_or("").trim().trim_end_matches('{').trim_end()
}

fn code(snippet: &str) -> String {
    format!("```clif\n{snippet}\n```")
}

fn definition_hover(document: &Document, resolution: &Resolution, def: usize) -> Option<String> {
    let text = document.text();
    let def = &resolution.defs[def];
    let functions = document.file().items.iter().enumerate().filter_map(|(index, item)| match item {
        Item::Function(f) => Some((index, f)),
        Item::Directive(_) => None,
    });
    match def.kind {
        NameKind::Function => {
            functions.map(|(_, f)| f).find(|f| f.name == Some(def.span)).map(|f| code(first_line(text, f.span)))
        }
        kind => {
            let (_, function) = functions.into_iter().find(|(index, _)| Some(*index) == def.scope)?;
            scoped_definition(document, function, kind, def.span)
        }
    }
}

fn scoped_definition(document: &Document, function: &Function, kind: NameKind, span: Span) -> Option<String> {
    let text = document.text();
    match kind {
        NameKind::Entity => {
            let decl = function.decls.iter().find(|d| Span::from(d.name) == span)?;
            Some(code(decl.span.text(text)))
        }
        NameKind::Block => {
            let block = function.blocks.iter().find(|b| Span::from(b.name) == span)?;
            Some(code(block.span.text(text)))
        }
        NameKind::Value => {
            for block in &function.blocks {
                if let Some(param) = block.params.iter().find(|p| Span::from(p.value) == span) {
                    let ty = param.ty.map(|t| t.text(text)).unwrap_or("?");
                    return Some(format!("{}\n\nparameter of `{}`, type `{ty}`", code(param.span.text(text)), block.name.text(text)));
                }
                if let Some(inst) = block.insts.iter().find(|i| i.results.iter().any(|r| Span::from(*r) == span)) {
                    let mut out = format!("defined by\n\n{}", code(inst.span.text(text)));
                    let inferred = inference::value_types(document);
                    let ty = inferred.get(&span.start).map(String::as_str).or(inst.type_suffix.first().map(|t| t.text(text)));
                    if let Some(ty) = ty {
                        out.push_str(&format!("\n\ntype `{ty}`"));
                    }
                    return Some(out);
                }
            }
            None
        }
        NameKind::Function => None,
    }
}

fn type_info(name: &str) -> Option<String> {
    tables::lookup(types::TYPES, name).map(named)
}

fn operand_info(opcode: Option<&str>, word: &str) -> Option<String> {
    let condition = if opcode == Some("fcmp") { codes::FLOAT_CC } else { codes::INT_CC };
    [condition, codes::TRAP_CODES, codes::MEM_FLAGS, codes::CALL_CONVS]
        .into_iter()
        .find_map(|table| tables::lookup(table, word))
        .map(named)
}

pub fn at(document: &Document, position: Position) -> Option<Hover> {
    let text = document.text();
    let offset = document.offset(position);
    let resolution = navigation::resolution(document);

    if let Some(target) = navigation::find(&resolution, offset) {
        let body = match target.def {
            Some(def) => definition_hover(document, &resolution, def),
            None if target.kind == NameKind::Function => Some(format!("external function `{}`", target.name)),
            None => None,
        };
        if let Some(body) = body {
            return Some(markdown(document, target.span, body));
        }
    }

    for item in &document.file().items {
        let Item::Function(function) = item else { continue };
        if !contains(function.span, offset) {
            continue;
        }
        let signature_types = function.signature.params.iter().chain(&function.signature.returns);
        for span in signature_types {
            if contains(*span, offset) {
                return type_word(document, *span, offset);
            }
        }
        for block in &function.blocks {
            for param in &block.params {
                if let Some(ty) = param.ty.filter(|t| contains(*t, offset)) {
                    return type_word(document, ty, offset);
                }
            }
            for inst in &block.insts {
                if !contains(inst.span, offset) {
                    continue;
                }
                let opcode = inst.opcode.map(|o| o.text(text));
                if let Some(span) = inst.opcode.filter(|o| contains(*o, offset)) {
                    return tables::opcode(span.text(text)).map(|info| markdown(document, span, opcode_hover(info)));
                }
                if let Some(span) = inst.type_suffix.iter().find(|t| contains(**t, offset)) {
                    return type_info(span.text(text)).map(|body| markdown(document, *span, body));
                }
                if let Some(token) = inst.operands.iter().find(|t| contains(Span::from(**t), offset)) {
                    let span = Span::from(*token);
                    return operand_info(opcode, span.text(text)).map(|body| markdown(document, span, body));
                }
            }
        }
    }
    None
}

fn type_word(document: &Document, span: Span, offset: usize) -> Option<Hover> {
    let text = document.text();
    let slice = span.text(text);
    let mut start = 0;
    for word in slice.split_inclusive(char::is_whitespace) {
        let trimmed = word.trim_end();
        let word_span = Span { start: span.start + start, end: span.start + start + trimmed.len() };
        if contains(word_span, offset) {
            return type_info(trimmed).map(|body| markdown(document, word_span, body));
        }
        start += word.len();
    }
    None
}
