use serde::Serialize;
use serde_json::Value;

use crate::analysis::{self, FunctionAnalysis};
use crate::cfg;
use crate::document::Document;
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

const VIEWS: [&str; 3] = ["cfg", "analysis", "stage"];

pub fn render(document: &Document, view: &str, function: Option<&str>, args: &Value) -> Result<View, String> {
    match view {
        "cfg" => cfg_view(document, function),
        "analysis" => analysis_view(document, function),
        "stage" => stage_view(document, function, args),
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
