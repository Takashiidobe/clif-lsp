use tower_lsp_server::ls_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range};

use crate::document::Document;
use crate::inference;
use crate::lexer::TokenKind;
use crate::syntax::{Function, Item};
use crate::tables;

fn hint(position: Position, label: String, kind: InlayHintKind, padding_right: bool) -> InlayHint {
    InlayHint {
        position,
        label: InlayHintLabel::String(label),
        kind: Some(kind),
        text_edits: None,
        tooltip: None,
        padding_left: Some(false),
        padding_right: Some(padding_right),
        data: None,
    }
}

fn branch_argument_hints(document: &Document, function: &Function, operands: &[crate::lexer::Token], out: &mut Vec<InlayHint>) {
    let text = document.text();
    let mut i = 0;
    while i < operands.len() {
        let target = operands[i];
        i += 1;
        if target.kind != TokenKind::Block || operands.get(i).is_none_or(|t| t.kind != TokenKind::LParen) {
            continue;
        }
        let params = function.blocks.iter().find(|b| b.name.text(text) == target.text(text)).map(|b| &b.params);
        let mut index = 0;
        i += 1;
        while let Some(token) = operands.get(i).filter(|t| t.kind != TokenKind::RParen) {
            if token.kind != TokenKind::Comma {
                let name = params.and_then(|p| p.get(index)).map(|p| p.value.text(text));
                if let Some(name) = name {
                    out.push(hint(document.position(token.start), format!("{name}:"), InlayHintKind::PARAMETER, true));
                }
                index += 1;
            }
            i += 1;
        }
    }
}

pub fn within(document: &Document, range: Range) -> Vec<InlayHint> {
    let text = document.text();
    let types = inference::value_types(document);
    let mut out = Vec::new();
    for item in &document.file().items {
        let Item::Function(function) = item else { continue };
        for inst in function.blocks.iter().flat_map(|b| &b.insts) {
            let suffix = inst.type_suffix.first().map(|t| t.text(text));
            for result in &inst.results {
                if let Some(ty) = types.get(&result.start).filter(|ty| Some(ty.as_str()) != suffix) {
                    out.push(hint(document.position(result.end), format!(": {ty}"), InlayHintKind::TYPE, false));
                }
            }
            let branches = inst.opcode.and_then(|o| tables::opcode(o.text(text))).is_some_and(|info| info.branch);
            if branches {
                branch_argument_hints(document, function, &inst.operands, &mut out);
            }
        }
    }
    out.retain(|h| range.start <= h.position && h.position <= range.end);
    out
}
