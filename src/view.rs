use serde::Serialize;

use crate::cfg;
use crate::document::Document;

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

const VIEWS: [&str; 1] = ["cfg"];

pub fn render(document: &Document, view: &str, function: Option<&str>) -> Result<View, String> {
    match view {
        "cfg" => cfg_view(document, function),
        _ => Err(format!("unknown view `{view}`, expected one of: {}", VIEWS.join(", "))),
    }
}

fn cfg_view(document: &Document, function: Option<&str>) -> Result<View, String> {
    let graphs = cfg::graphs(document);
    let graph = match function {
        Some(wanted) => graphs.iter().find(|g| g.name == wanted).ok_or_else(|| format!("no function `{wanted}`"))?,
        None => graphs.first().ok_or("the file has no functions")?,
    };
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
