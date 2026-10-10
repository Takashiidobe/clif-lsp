use serde::Serialize;

use crate::document::Document;
use crate::lexer::TokenKind;
use crate::syntax::{Function, Item, Span};

#[derive(Serialize)]
pub struct BlockNode {
    pub name: String,
    pub successors: Vec<usize>,
    pub reachable: bool,
    #[serde(skip)]
    pub span: Span,
}

#[derive(Serialize)]
pub struct Graph {
    pub name: String,
    pub entry: Option<usize>,
    pub blocks: Vec<BlockNode>,
    pub dot: String,
}

fn successors(document: &Document, function: &Function, block: &crate::syntax::Block) -> Vec<usize> {
    let text = document.text();
    let index_of = |name: &str| function.blocks.iter().position(|b| b.name.text(text) == name);
    let mut out: Vec<usize> = Vec::new();
    let mut push = |name: &str| {
        if let Some(index) = index_of(name).filter(|i| !out.contains(i)) {
            out.push(index);
        }
    };
    for inst in &block.insts {
        for token in &inst.operands {
            if token.kind == TokenKind::Block {
                push(token.text(text));
            }
        }
    }
    out
}

fn reach(blocks: &mut [BlockNode]) {
    let mut stack = vec![0];
    while let Some(index) = stack.pop() {
        if std::mem::replace(&mut blocks[index].reachable, true) {
            continue;
        }
        stack.extend(blocks[index].successors.iter().copied());
    }
}

fn render_dot(name: &str, blocks: &[BlockNode]) -> String {
    let mut out = format!("digraph \"{name}\" {{\n");
    for block in blocks {
        let style = if block.reachable { "" } else { " [style=dashed]" };
        out.push_str(&format!("    \"{}\"{style};\n", block.name));
        for &target in &block.successors {
            out.push_str(&format!("    \"{}\" -> \"{}\";\n", block.name, blocks[target].name));
        }
    }
    out.push_str("}\n");
    out
}

pub fn graphs(document: &Document) -> Vec<Graph> {
    let text = document.text();
    let mut out = Vec::new();
    for item in &document.file().items {
        let Item::Function(function) = item else { continue };
        let mut blocks: Vec<BlockNode> = function
            .blocks
            .iter()
            .map(|b| BlockNode {
                name: b.name.text(text).to_owned(),
                successors: successors(document, function, b),
                reachable: false,
                span: b.name.into(),
            })
            .collect();
        if !blocks.is_empty() {
            reach(&mut blocks);
        }
        let name = function.name.map_or_else(|| "function".to_owned(), |n| n.text(text).to_owned());
        let dot = render_dot(&name, &blocks);
        out.push(Graph { name, entry: (!blocks.is_empty()).then_some(0), blocks, dot });
    }
    out
}
