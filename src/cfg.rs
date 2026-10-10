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
    #[serde(skip)]
    pub lines: Vec<String>,
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

fn block_lines(text: &str, block: &crate::syntax::Block) -> Vec<String> {
    let params: Vec<String> = block
        .params
        .iter()
        .map(|p| match p.ty {
            Some(ty) => format!("{}: {}", p.value.text(text), ty.text(text)),
            None => p.value.text(text).to_owned(),
        })
        .collect();
    let header = if params.is_empty() { block.name.text(text).to_owned() } else { format!("{}({})", block.name.text(text), params.join(", ")) };
    let mut lines = vec![header];
    lines.extend(block.insts.iter().map(|inst| inst.span.text(text).lines().next().unwrap_or_default().trim().to_owned()));
    lines
}

const LINE_HEIGHT: usize = 20;
const CHAR_WIDTH: usize = 8;
const COLUMN_GAP: usize = 48;
const LAYER_GAP: usize = 72;
const MARGIN: usize = 24;
const LOOP_ROOM: usize = 90;

struct Placed {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

const CSS: &str = r#"
.cfg { position: relative; }
.cfg svg { position: absolute; left: 0; top: 0; }
.cfg path.edge { fill: none; stroke: var(--line); stroke-width: 1.4; marker-end: url(#arrow); }
.cfg marker path { fill: var(--line); }
.blk { position: absolute; box-sizing: border-box; border: 1px solid var(--border); background: var(--bg); padding: 4px 8px; white-space: pre; overflow: hidden; line-height: 20px; }
.blk b { display: block; }
.blk.entry { border-left: 4px solid var(--hl); }
.blk.dead { border-style: dashed; background: var(--head); color: var(--muted); }
.scroll { overflow: auto; }
"#;

pub fn html(name: &str, blocks: &[BlockNode]) -> String {
    use crate::html::{self, escape};

    let mut depth: Vec<Option<usize>> = vec![None; blocks.len()];
    if !blocks.is_empty() {
        depth[0] = Some(0);
        let mut queue = std::collections::VecDeque::from([0]);
        while let Some(index) = queue.pop_front() {
            for &next in &blocks[index].successors {
                if depth[next].is_none() {
                    depth[next] = Some(depth[index].unwrap_or(0) + 1);
                    queue.push_back(next);
                }
            }
        }
    }
    let last = depth.iter().flatten().max().copied().unwrap_or(0);
    let dead_layer = last + 1;
    let layer_of: Vec<usize> = depth.iter().map(|d| d.unwrap_or(dead_layer)).collect();
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); dead_layer + 1];
    for (index, &layer) in layer_of.iter().enumerate() {
        layers[layer].push(index);
    }
    layers.retain(|layer| !layer.is_empty());

    let size = |index: usize| {
        let chars = blocks[index].lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        ((chars * CHAR_WIDTH + 2 * CHAR_WIDTH + 4).max(120), blocks[index].lines.len().max(1) * LINE_HEIGHT + 8)
    };
    let layer_width = |layer: &[usize]| layer.iter().map(|&i| size(i).0).sum::<usize>() + COLUMN_GAP * layer.len().saturating_sub(1);
    let widest = layers.iter().map(|l| layer_width(l)).max().unwrap_or(0);
    let total_width = widest + 2 * MARGIN + LOOP_ROOM;

    let mut placed: Vec<Option<Placed>> = (0..blocks.len()).map(|_| None).collect();
    let mut y = MARGIN;
    for layer in &layers {
        let mut x = MARGIN + (widest - layer_width(layer)) / 2;
        let tallest = layer.iter().map(|&i| size(i).1).max().unwrap_or(0);
        for &index in layer {
            let (width, height) = size(index);
            placed[index] = Some(Placed { x, y, width, height });
            x += width + COLUMN_GAP;
        }
        y += tallest + LAYER_GAP;
    }
    let total_height = y - LAYER_GAP + MARGIN;

    let mut svg = format!("<svg width=\"{total_width}\" height=\"{total_height}\" viewBox=\"0 0 {total_width} {total_height}\">\n");
    svg.push_str("<defs><marker id=\"arrow\" markerWidth=\"9\" markerHeight=\"9\" refX=\"8\" refY=\"4.5\" orient=\"auto\"><path d=\"M0,0 L9,4.5 L0,9 z\"/></marker></defs>\n");
    for (index, block) in blocks.iter().enumerate() {
        let Some(from) = &placed[index] else { continue };
        for &target in &block.successors {
            let Some(to) = &placed[target] else { continue };
            let d = if layer_of[target] > layer_of[index] {
                let (x1, y1, x2, y2) = (from.x + from.width / 2, from.y + from.height, to.x + to.width / 2, to.y);
                format!("M{x1},{y1} C{x1},{} {x2},{} {x2},{y2}", y1 + LAYER_GAP / 2, y2 - LAYER_GAP / 2)
            } else if target == index {
                let (x, mid) = (from.x + from.width, from.y + from.height / 2);
                format!("M{x},{} C{},{} {},{} {x},{}", mid - 10, x + 55, mid - 35, x + 55, mid + 35, mid + 10)
            } else {
                let (x1, y1, x2, y2) = (from.x + from.width, from.y + from.height / 2, to.x + to.width, to.y + to.height / 2);
                let reach = LOOP_ROOM - 20;
                format!("M{x1},{y1} C{},{y1} {},{y2} {x2},{y2}", x1 + reach, x2 + reach)
            };
            svg.push_str(&format!("<path class=\"edge\" d=\"{d}\"/>\n"));
        }
    }
    svg.push_str("</svg>\n");

    let mut body = format!("<div class=\"scroll mono\"><div class=\"cfg\" style=\"width:{total_width}px;height:{total_height}px\">\n{svg}");
    for (index, block) in blocks.iter().enumerate() {
        let Some(at) = &placed[index] else { continue };
        let class = match (index == 0, block.reachable) {
            (true, _) => "blk entry",
            (false, true) => "blk",
            (false, false) => "blk dead",
        };
        let mut lines = block.lines.iter();
        let header = lines.next().map(|l| escape(l)).unwrap_or_default();
        let rest: Vec<String> = lines.map(|l| escape(l)).collect();
        body.push_str(&format!(
            "<div class=\"{class}\" style=\"left:{}px;top:{}px;width:{}px;height:{}px\"><b>{header}</b>{}</div>\n",
            at.x,
            at.y,
            at.width,
            at.height,
            rest.join("\n")
        ));
    }
    body.push_str("</div></div>\n");
    html::page(&format!("Control flow of {name}"), &[("bg", "reachable"), ("head", "unreachable (dashed)")], CSS, &body, "")
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
                lines: block_lines(text, b),
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
