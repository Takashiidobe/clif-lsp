use tower_lsp_server::ls_types::{
    ParameterInformation, ParameterLabel, Position, SignatureHelp, SignatureInformation,
};

use crate::completion;
use crate::document::Document;
use crate::syntax::{Function, Item};
use crate::tables::{self, codes};

fn labelled(label: &str, parts: &[&str], search_from: usize) -> Vec<ParameterInformation> {
    let mut cursor = search_from;
    parts
        .iter()
        .map(|part| {
            let start = cursor + label[cursor..].find(part).unwrap_or(0);
            cursor = start + part.len();
            ParameterInformation {
                label: ParameterLabel::LabelOffsets([start as u32, cursor as u32]),
                documentation: None,
            }
        })
        .collect()
}

fn top_level_commas(text: &str) -> u32 {
    let mut depth = 0i32;
    let mut commas = 0;
    for c in text.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas
}

fn active_parameter(first: Option<&str>, operands: &str) -> u32 {
    let commas = top_level_commas(operands);
    let words: Vec<&str> = operands.split_whitespace().collect();
    let trailing_space = operands.ends_with(char::is_whitespace);
    let shift = match first {
        Some("Cond") if commas == 0 => u32::from(!words.is_empty() && trailing_space),
        Some("MemFlags") if commas == 0 => {
            let past_flags = words.iter().enumerate().any(|(i, w)| {
                let partial = i + 1 == words.len() && !trailing_space;
                let is_flag = tables::lookup(codes::MEM_FLAGS, w).is_some();
                !is_flag && !(partial && codes::MEM_FLAGS.iter().any(|f| f.name.starts_with(w)))
            });
            u32::from(past_flags)
        }
        Some("Cond" | "MemFlags") => 1,
        _ => 0,
    };
    commas + shift
}

fn help(label: String, parameters: Vec<ParameterInformation>, active: u32, documentation: Option<String>) -> SignatureHelp {
    let last = parameters.len().saturating_sub(1) as u32;
    let active_parameter = if parameters.is_empty() { None } else { Some(active.min(last)) };
    SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: documentation.map(tower_lsp_server::ls_types::Documentation::String),
            parameters: Some(parameters),
            active_parameter,
        }],
        active_signature: Some(0),
        active_parameter,
    }
}

fn current_function(document: &Document, offset: usize) -> Option<&Function> {
    document.file().items.iter().find_map(|item| match item {
        Item::Function(f) if f.span.start <= offset && offset <= f.span.end => Some(f),
        _ => None,
    })
}

fn callee(document: &Document, offset: usize, name: &str, arguments: &str) -> Option<SignatureHelp> {
    let function = current_function(document, offset)?;
    let text = document.text();
    let decl_rhs = |wanted: &str| {
        function.decls.iter().find(|d| d.name.text(text) == wanted).map(|d| d.rhs.iter().map(|t| t.text(text)).collect::<Vec<_>>())
    };
    let rhs = decl_rhs(name)?;
    let sig_name = rhs.iter().rev().find(|w| w.starts_with("sig"))?;
    let signature = function.decls.iter().find(|d| d.name.text(text) == *sig_name)?;
    let words: Vec<&str> = signature.rhs.iter().map(|t| t.text(text)).collect();
    let inner_start = words.iter().position(|w| *w == "(")? + 1;
    let inner_end = words.iter().position(|w| *w == ")")?;
    let params: Vec<String> = words[inner_start..inner_end]
        .split(|w| *w == ",")
        .map(|p| p.concat())
        .filter(|p| !p.is_empty())
        .collect();
    let callee_name = rhs.iter().find(|w| w.starts_with('%')).copied().unwrap_or(name);
    let returns = words[inner_end + 1..].join(" ");
    let label = format!("{callee_name}({}){}{returns}", params.join(", "), if returns.is_empty() { "" } else { " " });
    let parts: Vec<&str> = params.iter().map(String::as_str).collect();
    Some(help(label.clone(), labelled(&label, &parts, callee_name.len()), top_level_commas(arguments), None))
}

pub fn at(document: &Document, position: Position) -> Option<SignatureHelp> {
    let text = document.text();
    let offset = document.offset(position);
    let line_start = document.offset(Position::new(position.line, 0));
    let prefix = &text[line_start..offset];
    if prefix.contains(';') {
        return None;
    }
    let body = completion::instruction_body(prefix)?;
    let (word, operands) = body.split_once(char::is_whitespace)?;
    let opcode = word.split('.').next().unwrap_or(word);
    let info = tables::opcode(opcode)?;

    if info.call {
        if let Some((name, arguments)) = operands.split_once('(') {
            let name = name.trim();
            if name.starts_with("fn") && !arguments.contains(')') {
                return callee(document, offset, name, arguments);
            }
        }
    }

    let label = info.signature;
    let rhs_start = label.rfind('=').map_or(0, |i| i + 1);
    let rhs = &label[rhs_start..];
    let opcode_end = rhs.find(info.name).map_or(0, |i| i + info.name.len());
    let parts: Vec<&str> = rhs[opcode_end..].split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
    let parameters = labelled(label, &parts, rhs_start + opcode_end);
    let doc = info.doc.lines().next().filter(|l| !l.is_empty()).map(str::to_owned);
    Some(help(label.to_owned(), parameters, active_parameter(parts.first().copied(), operands), doc))
}
