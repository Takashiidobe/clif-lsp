use crate::document::Document;
use crate::lexer::TokenKind;
use crate::syntax::{Function, Inst};
use crate::view::TreeNode;

pub struct Definition {
    pub block: String,
    pub text: String,
}

pub fn definition(document: &Document, function: &Function, value: &str) -> Option<Definition> {
    let text = document.text();
    function.blocks.iter().find_map(|block| {
        let block_name = block.name.text(text).to_owned();
        if let Some(param) = block.params.iter().find(|p| p.value.text(text) == value) {
            return Some(Definition { block: block_name, text: param.span.text(text).to_owned() });
        }
        block
            .insts
            .iter()
            .find(|inst| inst.results.iter().any(|r| r.text(text) == value))
            .map(|inst| Definition { block: block_name, text: inst_text(document, inst) })
    })
}

pub fn uses(document: &Document, function: &Function, value: &str, depth: usize) -> Vec<TreeNode> {
    if depth == 0 {
        return Vec::new();
    }
    let text = document.text();
    let mut nodes = Vec::new();
    for block in &function.blocks {
        for inst in block.insts.iter().filter(|inst| uses_value(text, inst, value)) {
            let children = inst.results.iter().flat_map(|r| uses(document, function, r.text(text), depth - 1)).collect();
            nodes.push(TreeNode { label: format!("{}: {}", block.name.text(text), inst_text(document, inst)), children });
        }
    }
    nodes
}

pub struct UseCount {
    pub value: String,
    pub block: String,
    pub definition: String,
    pub count: usize,
    pub first_user: Option<String>,
}

pub fn use_counts(document: &Document, function: &Function) -> Vec<UseCount> {
    let text = document.text();
    let mut counts: Vec<UseCount> = Vec::new();
    for block in &function.blocks {
        let block_name = block.name.text(text);
        for param in &block.params {
            counts.push(UseCount { value: param.value.text(text).to_owned(), block: block_name.to_owned(), definition: param.span.text(text).to_owned(), count: 0, first_user: None });
        }
        for inst in &block.insts {
            for result in &inst.results {
                counts.push(UseCount { value: result.text(text).to_owned(), block: block_name.to_owned(), definition: inst_text(document, inst), count: 0, first_user: None });
            }
        }
    }
    for block in &function.blocks {
        for inst in &block.insts {
            for operand in inst.operands.iter().filter(|t| t.kind == TokenKind::Value) {
                if let Some(entry) = counts.iter_mut().find(|c| c.value == operand.text(text)) {
                    entry.count += 1;
                    entry.first_user.get_or_insert_with(|| format!("{}: {}", block.name.text(text), inst_text(document, inst)));
                }
            }
        }
    }
    counts
}

fn uses_value(text: &str, inst: &Inst, value: &str) -> bool {
    inst.operands.iter().any(|t| t.kind == TokenKind::Value && t.text(text) == value)
}

fn inst_text(document: &Document, inst: &Inst) -> String {
    inst.span.text(document.text()).lines().next().unwrap_or_default().trim().to_owned()
}
