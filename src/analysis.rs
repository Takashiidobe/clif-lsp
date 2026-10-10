use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use crate::cfg::{self, BlockNode};
use crate::document::Document;
use crate::lexer::TokenKind;
use crate::syntax::{Function, Item};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockFacts {
    pub name: String,
    pub immediate_dominator: Option<String>,
    pub predecessors: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueLife {
    pub name: String,
    pub defined_in: String,
    pub live_in: Vec<String>,
    pub live_out: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionAnalysis {
    pub name: String,
    pub blocks: Vec<BlockFacts>,
    pub values: Vec<ValueLife>,
    #[serde(skip)]
    pub scope: usize,
}

fn predecessors(blocks: &[BlockNode]) -> Vec<Vec<usize>> {
    let mut preds = vec![Vec::new(); blocks.len()];
    for (index, block) in blocks.iter().enumerate() {
        for &next in &block.successors {
            preds[next].push(index);
        }
    }
    preds
}

fn reverse_postorder(blocks: &[BlockNode]) -> Vec<usize> {
    let mut seen = vec![false; blocks.len()];
    let mut order = Vec::new();
    let mut stack = vec![(0usize, 0usize)];
    seen[0] = true;
    while let Some((node, child)) = stack.pop() {
        match blocks[node].successors.get(child) {
            Some(&next) => {
                stack.push((node, child + 1));
                if !std::mem::replace(&mut seen[next], true) {
                    stack.push((next, 0));
                }
            }
            None => order.push(node),
        }
    }
    order.reverse();
    order
}

fn dominators(blocks: &[BlockNode], preds: &[Vec<usize>]) -> Vec<Option<usize>> {
    let mut idom: Vec<Option<usize>> = vec![None; blocks.len()];
    if blocks.is_empty() {
        return idom;
    }
    let order = reverse_postorder(blocks);
    let mut rank = vec![usize::MAX; blocks.len()];
    for (position, &block) in order.iter().enumerate() {
        rank[block] = position;
    }
    idom[0] = Some(0);
    let intersect = |idom: &[Option<usize>], mut a: usize, mut b: usize| {
        while a != b {
            while rank[a] > rank[b] {
                a = idom[a].expect("processed predecessor has a dominator");
            }
            while rank[b] > rank[a] {
                b = idom[b].expect("processed predecessor has a dominator");
            }
        }
        a
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &block in order.iter().skip(1) {
            let mut candidates = preds[block].iter().copied().filter(|p| idom[*p].is_some());
            let Some(first) = candidates.next() else { continue };
            let merged = candidates.fold(first, |acc, p| intersect(&idom, acc, p));
            if idom[block] != Some(merged) {
                idom[block] = Some(merged);
                changed = true;
            }
        }
    }
    idom[0] = None;
    idom
}

struct Facts {
    defined: BTreeSet<String>,
    used_first: BTreeSet<String>,
}

fn block_facts(document: &Document, block: &crate::syntax::Block) -> Facts {
    let text = document.text();
    let mut defined: BTreeSet<String> = block.params.iter().map(|p| p.value.text(text).to_owned()).collect();
    let mut used_first = BTreeSet::new();
    for inst in &block.insts {
        for token in inst.operands.iter().filter(|t| t.kind == TokenKind::Value) {
            let name = token.text(text);
            if !defined.contains(name) {
                used_first.insert(name.to_owned());
            }
        }
        defined.extend(inst.results.iter().map(|r| r.text(text).to_owned()));
    }
    Facts { defined, used_first }
}

fn liveness(facts: &[Facts], blocks: &[BlockNode]) -> (Vec<BTreeSet<String>>, Vec<BTreeSet<String>>) {
    let mut live_in = vec![BTreeSet::new(); blocks.len()];
    let mut live_out = vec![BTreeSet::new(); blocks.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for index in (0..blocks.len()).rev() {
            let out: BTreeSet<String> = blocks[index].successors.iter().flat_map(|&s| live_in[s].iter().cloned()).collect();
            let mut inn: BTreeSet<String> = out.difference(&facts[index].defined).cloned().collect();
            inn.extend(facts[index].used_first.iter().cloned());
            if inn != live_in[index] || out != live_out[index] {
                live_in[index] = inn;
                live_out[index] = out;
                changed = true;
            }
        }
    }
    (live_in, live_out)
}

fn analyze_function(document: &Document, scope: usize, function: &Function, graph: &cfg::Graph) -> FunctionAnalysis {
    let blocks = &graph.blocks;
    let preds = predecessors(blocks);
    let idom = dominators(blocks, &preds);
    let facts: Vec<Facts> = function.blocks.iter().map(|b| block_facts(document, b)).collect();
    let (live_in, live_out) = liveness(&facts, blocks);

    let names = |set: &mut dyn Iterator<Item = usize>| set.map(|i| blocks[i].name.clone()).collect::<Vec<_>>();
    let mut definitions: HashMap<&str, usize> = HashMap::new();
    for (index, fact) in facts.iter().enumerate() {
        for name in &fact.defined {
            definitions.entry(name).or_insert(index);
        }
    }
    let mut values: Vec<ValueLife> = definitions
        .iter()
        .map(|(&name, &home)| ValueLife {
            name: name.to_owned(),
            defined_in: blocks[home].name.clone(),
            live_in: names(&mut (0..blocks.len()).filter(|&b| live_in[b].contains(name))),
            live_out: names(&mut (0..blocks.len()).filter(|&b| live_out[b].contains(name))),
        })
        .collect();
    values.sort_by_key(|v| (v.name.len(), v.name.clone()));

    FunctionAnalysis {
        name: graph.name.clone(),
        blocks: blocks
            .iter()
            .enumerate()
            .map(|(index, block)| BlockFacts {
                name: block.name.clone(),
                immediate_dominator: idom[index].map(|d| blocks[d].name.clone()),
                predecessors: preds[index].iter().map(|&p| blocks[p].name.clone()).collect(),
            })
            .collect(),
        values,
        scope,
    }
}

pub fn analyze(document: &Document) -> Vec<FunctionAnalysis> {
    let graphs = cfg::graphs(document);
    let functions = document.file().items.iter().enumerate().filter_map(|(index, item)| match item {
        Item::Function(function) => Some((index, function)),
        Item::Directive(_) => None,
    });
    functions.zip(&graphs).map(|((index, function), graph)| analyze_function(document, index, function, graph)).collect()
}
