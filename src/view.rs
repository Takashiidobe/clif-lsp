use serde::Serialize;
use serde_json::Value;

use crate::analysis::{self, FunctionAnalysis};
use crate::cfg;
use crate::document::Document;
use crate::regalloc::{self, Event};
use crate::stages::{self, Stage};

pub const SCHEMA: u32 = 1;

#[derive(Serialize)]
pub struct View {
    pub schema: u32,
    pub view: String,
    pub function: String,
    pub title: String,
    pub sections: Vec<Section>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Section {
    Text { title: String, text: String },
    Dot { title: String, dot: String },
    Table { title: String, columns: Vec<String>, rows: Vec<Vec<String>> },
    Tree { title: String, nodes: Vec<TreeNode> },
    Diff { title: String, from: String, to: String, text: String },
}

#[derive(Serialize)]
pub struct TreeNode {
    pub label: String,
    pub children: Vec<TreeNode>,
}

const VIEWS: [&str; 5] = ["cfg", "analysis", "stage", "allocators", "regalloc"];

pub fn render(document: &Document, view: &str, function: Option<&str>, args: &Value) -> Result<View, String> {
    match view {
        "cfg" => cfg_view(document, function),
        "analysis" => analysis_view(document, function),
        "stage" => stage_view(document, function, args),
        "allocators" => allocators_view(document, function),
        "regalloc" => regalloc_view(document, function),
        _ => Err(format!("unknown view `{view}`, expected one of: {}", VIEWS.join(", "))),
    }
}

fn pick<'a, T>(items: &'a [T], name: impl Fn(&T) -> &str, function: Option<&str>) -> Result<&'a T, String> {
    match function {
        Some(wanted) => items.iter().find(|item| name(item) == wanted).ok_or_else(|| format!("no function `{wanted}`")),
        None => items.first().ok_or_else(|| "the file has no functions".to_owned()),
    }
}

fn cfg_view(document: &Document, function: Option<&str>) -> Result<View, String> {
    let graphs = cfg::graphs(document);
    let graph = pick(&graphs, |g| &g.name, function)?;
    let rows = graph
        .blocks
        .iter()
        .map(|block| {
            let successors: Vec<&str> = block.successors.iter().map(|&i| graph.blocks[i].name.as_str()).collect();
            vec![block.name.clone(), successors.join(", "), if block.reachable { "yes" } else { "no" }.to_owned()]
        })
        .collect();
    Ok(View {
        schema: SCHEMA,
        view: "cfg".to_owned(),
        function: graph.name.clone(),
        title: format!("Control flow of {}", graph.name),
        sections: vec![
            Section::Dot { title: "Graph".to_owned(), dot: graph.dot.clone() },
            Section::Table {
                title: "Blocks".to_owned(),
                columns: ["block", "successors", "reachable"].map(str::to_owned).to_vec(),
                rows,
            },
        ],
    })
}

fn analysis_view(document: &Document, function: Option<&str>) -> Result<View, String> {
    let all = analysis::analyze(document);
    let analysis = pick(&all, |a| &a.name, function)?;
    let blocks = analysis
        .blocks
        .iter()
        .map(|b| vec![b.name.clone(), b.immediate_dominator.clone().unwrap_or_default(), b.predecessors.join(", ")])
        .collect();
    let values = analysis
        .values
        .iter()
        .map(|v| vec![v.name.clone(), v.defined_in.clone(), v.live_in.join(", "), v.live_out.join(", ")])
        .collect();
    let columns = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect();
    Ok(View {
        schema: SCHEMA,
        view: "analysis".to_owned(),
        function: analysis.name.clone(),
        title: format!("Analysis of {}", analysis.name),
        sections: vec![
            Section::Tree { title: "Dominator tree".to_owned(), nodes: dominator_tree(analysis) },
            Section::Table { title: "Blocks".to_owned(), columns: columns(&["block", "immediate dominator", "predecessors"]), rows: blocks },
            Section::Table { title: "Values".to_owned(), columns: columns(&["value", "defined in", "live in", "live out"]), rows: values },
        ],
    })
}

fn dominator_tree(analysis: &FunctionAnalysis) -> Vec<TreeNode> {
    fn build(analysis: &FunctionAnalysis, name: &str) -> TreeNode {
        let children = analysis
            .blocks
            .iter()
            .filter(|b| b.immediate_dominator.as_deref() == Some(name))
            .map(|b| build(analysis, &b.name))
            .collect();
        TreeNode { label: name.to_owned(), children }
    }
    analysis.blocks.first().map(|entry| build(analysis, &entry.name)).into_iter().collect()
}

fn stage_view(document: &Document, function: Option<&str>, args: &Value) -> Result<View, String> {
    let name = args
        .get("stage")
        .and_then(Value::as_str)
        .ok_or("the stage view needs a `stage` argument, one of: parsed, optimized, vcode")?;
    let stage = Stage::parse(name).ok_or_else(|| format!("unknown stage `{name}`"))?;
    let rendered = stages::render(document, function, stage)?;
    Ok(View {
        schema: SCHEMA,
        view: "stage".to_owned(),
        title: format!("{name} {}", rendered.function),
        function: rendered.function,
        sections: vec![Section::Text { title: name.to_owned(), text: rendered.text }],
    })
}

const ALGORITHMS: [&str; 2] = ["backtracking", "single_pass"];

#[derive(Default)]
struct VcodeCounts {
    instructions: usize,
    moves: usize,
    stack_accesses: usize,
}

fn count_vcode(vcode: &str) -> VcodeCounts {
    let mut counts = VcodeCounts::default();
    for line in vcode.lines().filter(|l| l.starts_with("  ") && !l.trim_start().starts_with("unwind")) {
        let line = line.trim_start();
        let mnemonic = line.split_whitespace().next().unwrap_or_default();
        let touches_stack = ["(%rsp)", "(%rbp)", "[sp"].iter().any(|m| line.contains(m));
        counts.instructions += 1;
        counts.stack_accesses += usize::from(touches_stack);
        counts.moves += usize::from((mnemonic.starts_with("mov") || mnemonic.starts_with("fmov")) && !touches_stack && !line.contains('('));
    }
    counts
}

fn allocators_view(document: &Document, function: Option<&str>) -> Result<View, String> {
    let target = stages::target(document, function)?;
    let mut results = Vec::new();
    for algorithm in ALGORITHMS {
        let isa = stages::with_algorithm(&*target.isa, algorithm)?;
        let compiled = stages::compile(&target.function, &*isa).map_err(|e| if results.is_empty() { e } else { format!("{algorithm}: {e}") })?;
        results.push(compiled);
    }
    let counts: Vec<VcodeCounts> = results.iter().map(|c| count_vcode(&c.vcode)).collect();
    let row = |metric: &str, value: &dyn Fn(usize) -> usize| {
        vec![metric.to_owned(), value(0).to_string(), value(1).to_string()]
    };
    let rows = vec![
        row("code bytes", &|i| results[i].code_bytes),
        row("instructions", &|i| counts[i].instructions),
        row("moves", &|i| counts[i].moves),
        row("stack accesses", &|i| counts[i].stack_accesses),
    ];
    let mut columns = vec!["metric".to_owned()];
    columns.extend(ALGORITHMS.map(str::to_owned));
    let text = similar::TextDiff::from_lines(&results[0].vcode, &results[1].vcode)
        .unified_diff()
        .header(ALGORITHMS[0], ALGORITHMS[1])
        .to_string();
    Ok(View {
        schema: SCHEMA,
        view: "allocators".to_owned(),
        title: format!("Register allocators for {}", target.name),
        function: target.name,
        sections: vec![
            Section::Table { title: "Summary".to_owned(), columns, rows },
            Section::Diff { title: "vcode".to_owned(), from: ALGORITHMS[0].to_owned(), to: ALGORITHMS[1].to_owned(), text },
        ],
    })
}

fn regalloc_view(document: &Document, function: Option<&str>) -> Result<View, String> {
    let target = stages::target(document, function)?;
    let report = regalloc::analyze(&target)?;
    let columns = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect();
    let summary = vec![
        vec!["values placed".to_owned(), report.segments.iter().filter(|s| s.event == Event::Def).count().to_string()],
        vec!["spills".to_owned(), report.count(Event::Spill).to_string()],
        vec!["reloads".to_owned(), report.count(Event::Reload).to_string()],
        vec!["moves".to_owned(), report.count(Event::Move).to_string()],
        vec!["spill slots".to_owned(), report.spill_slots.to_string()],
        vec!["regalloc2 edits".to_owned(), report.edits.to_string()],
    ];
    let assignments = report
        .segments
        .iter()
        .map(|s| {
            let from = if s.event == Event::Def && report.is_param(&s.value) { "parameter".to_owned() } else { report.insts[s.from].clone() };
            vec![s.value.clone(), s.place.clone(), s.event.label().to_owned(), from, report.insts[s.to].clone()]
        })
        .collect();
    Ok(View {
        schema: SCHEMA,
        view: "regalloc".to_owned(),
        title: format!("Register allocation of {} (optimized CLIF)", report.name),
        function: report.name.clone(),
        sections: vec![
            Section::Table { title: "Summary".to_owned(), columns: columns(&["metric", "count"]), rows: summary },
            Section::Table { title: "Assignments".to_owned(), columns: columns(&["value", "location", "event", "from", "until"]), rows: assignments },
            Section::Dot { title: "Allocation graph".to_owned(), dot: report.dot() },
        ],
    })
}
