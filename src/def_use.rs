use crate::document::Document;
use crate::html::{self, escape};
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

pub fn uses_value(text: &str, inst: &Inst, value: &str) -> bool {
    inst.operands.iter().any(|t| t.kind == TokenKind::Value && t.text(text) == value)
}

pub fn inst_text(document: &Document, inst: &Inst) -> String {
    inst.span.text(document.text()).lines().next().unwrap_or_default().trim().to_owned()
}

const CSS: &str = r#"
.card { border: 1px solid var(--border); margin: 0 0 16px; max-width: 900px; }
.card .head { background: var(--head); padding: 4px 12px; font-weight: 600; }
.card .row { padding: 0 12px; border-top: 1px solid var(--border); }
.card .row span, .tree summary span { color: var(--muted); margin-right: 8px; }
.tree { max-width: 900px; padding: 0 12px; border: 1px solid var(--border); }
.tree details > details, .tree details > div { margin-left: 20px; }
.tree details > div { padding-left: 15px; }
.tree summary:hover, .tree div:hover { outline: 2px solid var(--hl); outline-offset: -2px; }
table { border-collapse: collapse; margin: 0 0 16px; }
th, td { border: 1px solid var(--border); padding: 0 8px; height: var(--row); white-space: pre; text-align: left; }
th { background: var(--head); }
tr:hover td { outline: 2px solid var(--hl); outline-offset: -2px; }
h2 { font-size: 14px; margin: 16px 0 8px; color: var(--muted); }
"#;

fn node_html(node: &TreeNode, out: &mut String) {
    let (block, inst) = node.label.split_once(": ").unwrap_or(("", node.label.as_str()));
    let label = format!("<span>{}</span>{}", escape(block), escape(inst));
    if node.children.is_empty() {
        out.push_str(&format!("<div>{label}</div>\n"));
        return;
    }
    out.push_str(&format!("<details open><summary>{label}</summary>\n"));
    for child in &node.children {
        node_html(child, out);
    }
    out.push_str("</details>\n");
}

pub fn html(function: &str, value: &str, definition: &Definition, nodes: &[TreeNode]) -> String {
    let mut body = format!(
        "<div class=\"card mono\"><div class=\"head\">{} in {}</div><div class=\"row\"><span>{}</span>{}</div></div>\n<h2>Uses</h2>\n",
        escape(value),
        escape(function),
        escape(&definition.block),
        escape(&definition.text)
    );
    if nodes.is_empty() {
        body.push_str("<p>No uses.</p>\n");
    } else {
        body.push_str("<div class=\"tree mono\">\n");
        for node in nodes {
            node_html(node, &mut body);
        }
        body.push_str("</div>\n");
    }
    html::page(&format!("Uses of {value} in {function}"), &[], CSS, &body, "")
}

fn counts_table(title: &str, columns: &[&str], rows: &[Vec<&str>]) -> String {
    let mut out = format!("<h2>{} ({})</h2>\n", escape(title), rows.len());
    if rows.is_empty() {
        out.push_str("<p>None.</p>\n");
        return out;
    }
    out.push_str("<table class=\"mono\"><tr>");
    for column in columns {
        out.push_str(&format!("<th>{}</th>", escape(column)));
    }
    out.push_str("</tr>\n");
    for row in rows {
        out.push_str("<tr>");
        for cell in row {
            out.push_str(&format!("<td>{}</td>", escape(cell)));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

pub fn counts_html(function: &str, counts: &[UseCount]) -> String {
    let dead: Vec<Vec<&str>> = counts.iter().filter(|c| c.count == 0).map(|c| vec![c.value.as_str(), c.block.as_str(), c.definition.as_str()]).collect();
    let single: Vec<Vec<&str>> = counts
        .iter()
        .filter(|c| c.count == 1)
        .map(|c| vec![c.value.as_str(), c.block.as_str(), c.definition.as_str(), c.first_user.as_deref().unwrap_or_default()])
        .collect();
    let body = counts_table("Dead values", &["value", "block", "definition"], &dead) + &counts_table("Single-use values", &["value", "block", "definition", "used by"], &single);
    html::page(&format!("Dead and single-use values of {function}"), &[], CSS, &body, "")
}
