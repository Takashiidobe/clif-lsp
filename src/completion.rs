use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, Position};

use crate::document::Document;
use crate::navigation;
use crate::resolve::NameKind;
use crate::syntax::Item;
use crate::tables::{self, NameInfo, OpcodeInfo, codes, opcodes, types};

fn named(table: &[NameInfo], kind: CompletionItemKind) -> impl Iterator<Item = CompletionItem> + '_ {
    table.iter().map(move |info| CompletionItem {
        label: info.name.to_owned(),
        kind: Some(kind),
        detail: Some(info.doc.lines().next().unwrap_or("").to_owned()),
        ..CompletionItem::default()
    })
}

#[derive(Default)]
struct Stops {
    tab: usize,
    values: usize,
}

impl Stops {
    fn text(&mut self, sample: &str) -> String {
        self.tab += 1;
        format!("${{{}:{sample}}}", self.tab)
    }

    fn choice(&mut self, table: &[NameInfo]) -> String {
        self.tab += 1;
        let names: Vec<&str> = table.iter().map(|info| info.name).collect();
        format!("${{{}|{}|}}", self.tab, names.join(","))
    }

    fn value(&mut self) -> String {
        let sample = format!("v{}", self.values);
        self.values += 1;
        self.text(&sample)
    }
}

fn snippet(info: &OpcodeInfo) -> Option<String> {
    let mut stops = Stops::default();
    let suffix = if info.doc.contains("(controlling type variable)") { format!(".{}", stops.text("i32")) } else { String::new() };
    let operands = match info.name {
        "call" => format!("{}({})", stops.text("fn0"), stops.value()),
        "call_indirect" => format!("{}, {}({})", stops.text("sig0"), stops.value(), stops.value()),
        _ => {
            let rhs = info.signature.rsplit('=').next()?.trim().strip_prefix(info.name)?;
            let mut parts = Vec::new();
            for word in rhs.split(',').map(str::trim).filter(|w| !w.is_empty()) {
                parts.push(match word {
                    "MemFlags" => continue,
                    "Cond" => stops.choice(if info.name.starts_with('f') { codes::FLOAT_CC } else { codes::INT_CC }),
                    "code" => stops.text("user1"),
                    "N" | "Offset" | "Idx" => stops.text("0"),
                    "FN" | "SIG" | "GV" | "SS" | "DSS" => stops.text(&format!("{}0", word.to_lowercase())),
                    w if w.starts_with("block") => stops.text("block0"),
                    "args" | "mask" => return None,
                    w if w.chars().any(char::is_uppercase) => return None,
                    _ => stops.value(),
                });
            }
            parts.join(", ")
        }
    };
    (stops.tab > 0).then(|| format!("{}{suffix} {operands}", info.name))
}

fn opcode_items(snippets: bool) -> Vec<CompletionItem> {
    opcodes::OPCODES
        .iter()
        .map(|info| {
            let snippet = snippets.then(|| snippet(info)).flatten();
            CompletionItem {
                label: info.name.to_owned(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(info.signature.to_owned()),
                documentation: Some(Documentation::String(info.doc.to_owned())),
                insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                insert_text: snippet,
                ..CompletionItem::default()
            }
        })
        .collect()
}

fn type_items() -> Vec<CompletionItem> {
    named(types::TYPES, CompletionItemKind::TYPE_PARAMETER).collect()
}

fn scope_items(document: &Document, offset: usize) -> Vec<CompletionItem> {
    let resolution = navigation::resolution(document);
    let current = document
        .file()
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, Item::Function(f) if f.span.start <= offset))
        .map(|(index, _)| index)
        .last();
    resolution
        .defs
        .iter()
        .filter(|d| d.kind == NameKind::Function || (d.scope.is_some() && d.scope == current))
        .map(|d| CompletionItem {
            label: d.name.clone(),
            kind: Some(match d.kind {
                NameKind::Value => CompletionItemKind::VARIABLE,
                NameKind::Block => CompletionItemKind::MODULE,
                NameKind::Entity => CompletionItemKind::FIELD,
                NameKind::Function => CompletionItemKind::FUNCTION,
            }),
            detail: Some(d.kind.label().to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

fn header_items(prefix: &str) -> Vec<CompletionItem> {
    let partial = prefix.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
    match partial.trim_end().chars().last() {
        Some('(' | ',' | ':' | '>') => type_items(),
        _ => Vec::new(),
    }
}

fn operand_items(document: &Document, offset: usize, opcode: &str, operands: &str) -> Vec<CompletionItem> {
    let Some(info) = tables::opcode(opcode) else { return Vec::new() };
    let signature = info.signature.rsplit('=').next().unwrap_or("").trim();
    let operand_words: Vec<&str> = operands.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()).collect();
    let first_operand = signature.split_whitespace().nth(1).unwrap_or("").trim_end_matches(',');
    let before_value = operand_words.iter().all(|w| tables::lookup(codes::MEM_FLAGS, w).is_some());

    let mut items = Vec::new();
    match first_operand {
        "Cond" if operand_words.is_empty() => {
            let table = if opcode.starts_with('f') { codes::FLOAT_CC } else { codes::INT_CC };
            items.extend(named(table, CompletionItemKind::ENUM_MEMBER));
        }
        "MemFlags" if before_value => items.extend(named(codes::MEM_FLAGS, CompletionItemKind::ENUM_MEMBER)),
        _ => {}
    }
    let takes_trap_code = signature.ends_with("code") && (opcode == "trap" || operands.contains(','));
    if takes_trap_code {
        items.extend(named(codes::TRAP_CODES, CompletionItemKind::ENUM_MEMBER));
    }
    items.extend(scope_items(document, offset));
    items
}

pub fn instruction_body(prefix: &str) -> Option<&str> {
    let trimmed = prefix.trim_start();
    match trimmed.split_once('=') {
        Some((_, rest)) => Some(rest.trim_start()),
        None if prefix.starts_with(char::is_whitespace) => Some(trimmed),
        None => None,
    }
}

pub fn at(document: &Document, position: Position, snippets: bool) -> Vec<CompletionItem> {
    let text = document.text();
    let offset = document.offset(position);
    let line_start = document.offset(Position::new(position.line, 0));
    let prefix = &text[line_start..offset];
    if prefix.contains(';') {
        return Vec::new();
    }
    let trimmed = prefix.trim_start();
    if trimmed.starts_with("function") || (trimmed.starts_with("block") && trimmed.contains('(')) {
        return header_items(prefix);
    }
    let Some(body) = instruction_body(prefix) else { return Vec::new() };
    match body.split_once(char::is_whitespace) {
        None => match body.split_once('.') {
            Some((opcode, _)) if tables::opcode(opcode).is_some() => type_items(),
            Some(_) => Vec::new(),
            None => opcode_items(snippets),
        },
        Some((word, operands)) => {
            let opcode = word.split('.').next().unwrap_or(word);
            operand_items(document, offset, opcode, operands)
        }
    }
}
