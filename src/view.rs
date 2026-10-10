use serde::Serialize;
use serde_json::Value;

use crate::analysis::{self, FunctionAnalysis};
use crate::cfg;
use crate::def_use;
use crate::document::Document;
use crate::egraph;
use crate::provenance::{self, Status};
use crate::regalloc::{self, Event};
use crate::stages::{self, Stage};
use crate::syntax::{Function, Item};

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
    Html { title: String, html: String },
    Table { title: String, columns: Vec<String>, rows: Vec<Vec<String>> },
    Tree { title: String, nodes: Vec<TreeNode> },
    Diff { title: String, from: String, to: String, text: String },
}

#[derive(Serialize)]
pub struct TreeNode {
    pub label: String,
    pub children: Vec<TreeNode>,
}

const VIEWS: [&str; 8] = ["cfg", "analysis", "def_use", "stage", "allocators", "regalloc", "provenance", "egraph"];

pub fn render(document: &Document, view: &str, function: Option<&str>, args: &Value) -> Result<View, String> {
    match view {
        "cfg" => cfg_view(document, function),
        "analysis" => analysis_view(document, function),
        "def_use" => def_use_view(document, function, args),
        "stage" => stage_view(document, function, args),
        "allocators" => allocators_view(document, function),
        "regalloc" => regalloc_view(document, function),
        "provenance" => provenance_view(document, function),
        "egraph" => egraph_view(document, function),
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
            Section::Html { title: "Graph".to_owned(), html: cfg::html(&graph.name, &graph.blocks) },
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

fn def_use_view(document: &Document, function: Option<&str>, args: &Value) -> Result<View, String> {
    let value = args.get("value").and_then(Value::as_str).ok_or("the def_use view needs a `value` argument, such as `v2`")?;
    let depth = match args.get("depth") {
        None => 1,
        Some(depth) => depth.as_u64().filter(|&d| d >= 1).ok_or("`depth` must be a positive integer")? as usize,
    };
    let text = document.text();
    let functions: Vec<&Function> = document
        .file()
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(function) => Some(function),
            Item::Directive(_) => None,
        })
        .collect();
    let function_name = |f: &Function| f.name.map_or("", |name| name.text(text)).to_owned();
    let function = pick(&functions, |f| f.name.map_or("", |name| name.text(text)), function)?;
    let name = function_name(function);
    let definition = def_use::definition(document, function, value).ok_or_else(|| format!("no value `{value}` in {name}"))?;
    Ok(View {
        schema: SCHEMA,
        view: "def_use".to_owned(),
        title: format!("Uses of {value} in {name}"),
        function: name,
        sections: vec![
            Section::Table {
                title: "Definition".to_owned(),
                columns: ["value", "block", "instruction"].map(str::to_owned).to_vec(),
                rows: vec![vec![value.to_owned(), definition.block, definition.text]],
            },
            Section::Tree { title: "Uses".to_owned(), nodes: def_use::uses(document, function, value, depth) },
        ],
    })
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

fn provenance_view(document: &Document, function: Option<&str>) -> Result<View, String> {
    let target = stages::target(document, function)?;
    let report = provenance::analyze(&target)?;
    let columns = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect();
    let summary = vec![
        vec!["input instructions".to_owned(), report.inputs.to_string()],
        vec!["unchanged".to_owned(), report.count(Status::Unchanged).to_string()],
        vec!["changed".to_owned(), report.count(Status::Changed).to_string()],
        vec!["new, no provenance available".to_owned(), report.count(Status::New).to_string()],
        vec!["no longer present".to_owned(), report.removed.len().to_string()],
    ];
    let instructions = report
        .rows
        .iter()
        .map(|row| {
            let origin = row.origin.clone().unwrap_or_else(|| "no provenance available".to_owned());
            vec![row.block.clone(), row.text.clone(), row.status.label().to_owned(), origin]
        })
        .collect();
    let removed = report.removed.iter().map(|text| vec![text.clone()]).collect();
    Ok(View {
        schema: SCHEMA,
        view: "provenance".to_owned(),
        title: format!("Optimization provenance of {}", report.name),
        function: report.name.clone(),
        sections: vec![
            Section::Table { title: "Summary".to_owned(), columns: columns(&["metric", "count"]), rows: summary },
            Section::Html { title: "Provenance map".to_owned(), html: report.html() },
            Section::Table { title: "Optimized instructions".to_owned(), columns: columns(&["block", "instruction", "status", "origin"]), rows: instructions },
            Section::Table { title: "No longer present".to_owned(), columns: columns(&["input instruction"]), rows: removed },
        ],
    })
}

fn egraph_view(document: &Document, function: Option<&str>) -> Result<View, String> {
    let target = stages::target(document, function)?;
    let report = egraph::analyze(&target)?;
    let columns = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect();
    let stat = |name: &str| report.stats.iter().find(|(key, _)| key == name).map_or(0, |(_, count)| *count);
    let summary = vec![
        vec!["input values".to_owned(), report.classes.len().to_string()],
        vec!["with alternatives".to_owned(), report.with_alternatives().count().to_string()],
        vec!["rewritten by the extractor".to_owned(), report.rewritten().to_string()],
        vec!["rewrite rule invocations".to_owned(), stat("rewrite_rule_invoked").to_string()],
        vec!["rewrite rule results".to_owned(), stat("rewrite_rule_results").to_string()],
    ];
    let mut forms = Vec::new();
    for class in report.with_alternatives() {
        for member in &class.members {
            let rules: Vec<String> = member.rules.iter().map(|r| format!("{}:{}", r.file, r.line)).collect();
            forms.push(vec![
                class.input.clone(),
                member.text.clone(),
                member.cost.map_or("unknown".to_owned(), |cost| cost.to_string()),
                if member.chosen { "yes" } else { "no" }.to_owned(),
                rules.join(", "),
            ]);
        }
    }
    Ok(View {
        schema: SCHEMA,
        view: "egraph".to_owned(),
        title: format!("E-graph of {}", report.name),
        function: report.name.clone(),
        sections: vec![
            Section::Table { title: "Summary".to_owned(), columns: columns(&["metric", "count"]), rows: summary },
            Section::Html { title: "Equivalent forms".to_owned(), html: report.html() },
            Section::Table { title: "Alternatives".to_owned(), columns: columns(&["input", "form", "cost", "chosen", "rule"]), rows: forms },
        ],
    })
}

fn regalloc_view(document: &Document, function: Option<&str>) -> Result<View, String> {
    let target = stages::target(document, function)?;
    let report = regalloc::analyze(&target)?;
    let columns = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect();
    let summary = vec![
        vec!["values placed".to_owned(), report.segments.iter().filter(|s| s.event == Event::Def).count().to_string()],
        vec!["max register pressure".to_owned(), report.max_pressure().to_string()],
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
    let pressure: Vec<Vec<String>> = (0..report.insts.len())
        .map(|inst| {
            let live = report.in_registers(inst);
            vec![report.insts[inst].clone(), live.len().to_string(), live.join(", ")]
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
            Section::Table { title: "Register pressure".to_owned(), columns: columns(&["instruction", "registers", "values"]), rows: pressure },
            Section::Html { title: "Allocation map".to_owned(), html: report.html() },
        ],
    })
}
