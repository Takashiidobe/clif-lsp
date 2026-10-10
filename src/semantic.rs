use tower_lsp_server::ls_types::{SemanticToken, SemanticTokenType, SemanticTokensLegend};

use crate::document::Document;
use crate::lexer::{self, TokenKind};
use crate::syntax::{Item, Span};
use crate::tables::{self, codes, types};

const KEYWORD: u32 = 0;
const TYPE: u32 = 1;
const VARIABLE: u32 = 2;
const NAMESPACE: u32 = 3;
const FUNCTION: u32 = 4;
const PROPERTY: u32 = 5;
const COMMENT: u32 = 6;
const NUMBER: u32 = 7;
const ENUM_MEMBER: u32 = 8;
const STRING: u32 = 9;

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::KEYWORD,
            SemanticTokenType::TYPE,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::NAMESPACE,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::COMMENT,
            SemanticTokenType::NUMBER,
            SemanticTokenType::ENUM_MEMBER,
            SemanticTokenType::STRING,
        ],
        token_modifiers: Vec::new(),
    }
}

fn keyword_spans(document: &Document) -> Vec<Span> {
    let mut spans = Vec::new();
    for item in &document.file().items {
        match item {
            Item::Directive(directive) => spans.push(directive.keyword),
            Item::Function(function) => {
                spans.push(Span { start: function.span.start, end: function.span.start + "function".len() });
                for block in &function.blocks {
                    spans.extend(block.insts.iter().filter_map(|inst| inst.opcode));
                }
            }
        }
    }
    spans
}

pub fn tokens(document: &Document) -> Vec<SemanticToken> {
    let text = document.text();
    let keywords = keyword_spans(document);
    let mut out = Vec::new();
    let (mut last_line, mut last_start) = (0, 0);

    for token in lexer::lex(text) {
        let word = token.text(text);
        let class = match token.kind {
            TokenKind::Comment => COMMENT,
            TokenKind::Value => VARIABLE,
            TokenKind::Block => NAMESPACE,
            TokenKind::Entity => PROPERTY,
            TokenKind::FuncName => FUNCTION,
            TokenKind::Number => NUMBER,
            TokenKind::String => STRING,
            TokenKind::Ident if keywords.contains(&Span::from(token)) => KEYWORD,
            TokenKind::Ident if tables::lookup(types::TYPES, word).is_some() => TYPE,
            TokenKind::Ident if [codes::INT_CC, codes::FLOAT_CC, codes::TRAP_CODES, codes::MEM_FLAGS, codes::CALL_CONVS]
                .iter()
                .any(|table| tables::lookup(table, word).is_some()) =>
            {
                ENUM_MEMBER
            }
            _ => continue,
        };
        let start = document.position(token.start);
        let end = document.position(token.end);
        if start.line != end.line {
            continue;
        }
        let delta_line = start.line - last_line;
        let delta_start = if delta_line == 0 { start.character - last_start } else { start.character };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length: end.character - start.character,
            token_type: class,
            token_modifiers_bitset: 0,
        });
        (last_line, last_start) = (start.line, start.character);
    }
    out
}
