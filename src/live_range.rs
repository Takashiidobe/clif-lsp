use crate::analysis::FunctionAnalysis;
use crate::def_use::{inst_text, uses_value};
use crate::document::Document;
use crate::html::{self, escape};
use crate::syntax::Item;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Def,
    Live,
    LastUse,
}

pub struct Row {
    pub text: String,
    pub header: bool,
}

#[derive(Default)]
pub struct Map {
    pub name: String,
    pub rows: Vec<Row>,
    pub columns: Vec<String>,
    pub cells: Vec<Vec<Cell>>,
}

pub fn map(document: &Document, analysis: &FunctionAnalysis) -> Map {
    let Item::Function(function) = &document.file().items[analysis.scope] else { return Map::default() };
    let text = document.text();
    let mut rows = Vec::new();
    let mut bases = Vec::new();
    for block in &function.blocks {
        bases.push(rows.len());
        let params: Vec<&str> = block.params.iter().map(|p| p.span.text(text)).collect();
        let header = if params.is_empty() { block.name.text(text).to_owned() } else { format!("{}({})", block.name.text(text), params.join(", ")) };
        rows.push(Row { text: header, header: true });
        rows.extend(block.insts.iter().map(|inst| Row { text: inst_text(document, inst), header: false }));
    }

    let mut cells = vec![vec![Cell::Empty; analysis.values.len()]; rows.len()];
    for (block, &base) in function.blocks.iter().zip(&bases) {
        let block_name = block.name.text(text);
        let last_row = base + block.insts.len();
        for (column, value) in analysis.values.iter().enumerate() {
            let name = value.name.as_str();
            let live_in = value.live_in.iter().any(|b| b == block_name);
            let live_out = value.live_out.iter().any(|b| b == block_name);
            let defined_at = if block.params.iter().any(|p| p.value.text(text) == name) {
                Some(base)
            } else {
                block.insts.iter().position(|inst| inst.results.iter().any(|r| r.text(text) == name)).map(|k| base + 1 + k)
            };
            let start = match defined_at {
                Some(row) => row,
                None if live_in => base,
                None => continue,
            };
            let last_use = block.insts.iter().rposition(|inst| uses_value(text, inst, name)).map(|k| base + 1 + k);
            let end = if live_out { last_row } else { last_use.unwrap_or(start).max(start) };
            for row in start..=end {
                cells[row][column] = Cell::Live;
            }
            if defined_at.is_some() {
                cells[start][column] = Cell::Def;
            }
            if !live_out && end > start {
                cells[end][column] = Cell::LastUse;
            }
        }
    }

    Map { name: analysis.name.clone(), rows, columns: analysis.values.iter().map(|v| v.name.clone()).collect(), cells }
}

const CSS: &str = r#"
.scroll { overflow: auto; max-height: calc(100vh - 110px); }
table { border-collapse: collapse; }
th, td { border: 1px solid var(--border); padding: 0 8px; height: var(--row); white-space: pre; text-align: center; }
th { background: var(--head); position: sticky; top: 0; z-index: 2; }
th:first-child, td:first-child { position: sticky; left: 0; text-align: left; background: var(--bg); z-index: 1; }
th:first-child { z-index: 3; background: var(--head); }
tr.block td:first-child { background: var(--head); font-weight: 600; }
td.def { background: var(--new); }
td.live { background: var(--reg); }
td.last { background: var(--stack); }
td.pressure { text-align: left; min-width: 160px; }
td.pressure i { display: inline-block; height: 10px; background: var(--hl); opacity: 0.55; margin-right: 8px; vertical-align: -1px; }
td.pressure.peak b { font-weight: 700; }
"#;

impl Map {
    pub fn live_at(&self, row: usize) -> Vec<&str> {
        self.columns.iter().zip(&self.cells[row]).filter(|(_, cell)| **cell != Cell::Empty).map(|(name, _)| name.as_str()).collect()
    }

    pub fn max_pressure(&self) -> usize {
        (0..self.rows.len()).map(|row| self.live_at(row).len()).max().unwrap_or(0)
    }

    pub fn html(&self) -> String {
        let peak = self.max_pressure();
        let mut body = String::from("<div class=\"scroll mono\"><table>\n<tr><th>instruction</th>");
        for column in &self.columns {
            body.push_str(&format!("<th>{}</th>", escape(column)));
        }
        body.push_str("<th>values live</th></tr>\n");
        for (index, row) in self.rows.iter().enumerate() {
            let class = if row.header { " class=\"block\"" } else { "" };
            body.push_str(&format!("<tr{class}><td>{}</td>", escape(&row.text)));
            for cell in &self.cells[index] {
                let (class, mark) = match cell {
                    Cell::Empty => ("", ""),
                    Cell::Def => ("def", "def"),
                    Cell::Live => ("live", "|"),
                    Cell::LastUse => ("last", "use"),
                };
                body.push_str(&format!("<td class=\"{class}\">{mark}</td>"));
            }
            let live = self.live_at(index).len();
            let (class, count) = if live == peak && peak > 0 { ("pressure peak", format!("<b>{live}</b>")) } else { ("pressure", live.to_string()) };
            body.push_str(&format!("<td class=\"{class}\"><i style=\"width:{}px\"></i>{count}</td></tr>\n", live * 10));
        }
        body.push_str("</table></div>\n");
        let legend = [("new", "defined"), ("reg", "live"), ("stack", "last use")];
        html::page(&format!("Live ranges of {}", self.name), &legend, CSS, &body, "")
    }
}
