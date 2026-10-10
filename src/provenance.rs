use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::ir::{Function, Inst, SourceLoc};
use cranelift_codegen::Context;

use crate::html::{self, escape, ROW_HEIGHT};
use crate::stages::{self, Target};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Unchanged,
    Changed,
    New,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Unchanged => "unchanged",
            Status::Changed => "changed",
            Status::New => "new",
        }
    }
}

pub struct Row {
    pub block: String,
    pub text: String,
    pub status: Status,
    pub origin: Option<String>,
    pub origin_index: Option<usize>,
}

pub struct Report {
    pub name: String,
    pub inputs: usize,
    pub originals: Vec<String>,
    pub survived: Vec<bool>,
    pub rows: Vec<Row>,
    pub removed: Vec<String>,
}

fn insts_of(function: &Function) -> Vec<Inst> {
    function.layout.blocks().flat_map(|block| function.layout.block_insts(block)).collect()
}

pub fn analyze(target: &Target) -> Result<Report, String> {
    let mut context = Context::for_function(target.function.clone());
    let originals: Vec<String> = insts_of(&context.func).iter().map(|&inst| context.func.dfg.display_inst(inst).to_string()).collect();
    for (index, inst) in insts_of(&context.func).into_iter().enumerate() {
        context.func.set_srcloc(inst, SourceLoc::new(index as u32 + 1));
    }
    context.optimize(&*target.isa, &mut ControlPlane::default()).map_err(|e| stages::describe(&e))?;

    let mut survived = vec![false; originals.len()];
    let mut rows = Vec::new();
    for block in context.func.layout.blocks() {
        for inst in context.func.layout.block_insts(block) {
            let text = context.func.dfg.display_inst(inst).to_string();
            let loc = context.func.srcloc(inst);
            let origin_index = (!loc.is_default()).then(|| loc.bits() as usize - 1).filter(|&index| index < originals.len());
            let status = match origin_index {
                None => Status::New,
                Some(index) => {
                    survived[index] = true;
                    if originals[index] == text { Status::Unchanged } else { Status::Changed }
                }
            };
            let origin = origin_index.map(|index| originals[index].clone());
            rows.push(Row { block: block.to_string(), text, status, origin, origin_index });
        }
    }
    let removed = originals.iter().zip(&survived).filter(|&(_, &kept)| !kept).map(|(text, _)| text.clone()).collect();
    Ok(Report { name: target.name.clone(), inputs: originals.len(), originals, survived, rows, removed })
}

const LINK_WIDTH: usize = 200;

const CSS: &str = r#"
.map { display: flex; align-items: flex-start; overflow-x: auto; padding-bottom: 8px; }
.col { flex: none; }
.head { height: var(--row); font-weight: 600; color: var(--muted); }
.row { height: var(--row); box-sizing: border-box; padding: 0 8px; border: 1px solid var(--border); border-bottom: 0; white-space: pre; }
.row:last-child { border-bottom: 1px solid var(--border); }
.row.removed { background: var(--removed); }
.row.changed { background: var(--changed); }
.row.new { background: var(--new); }
.row .block { color: var(--muted); margin-right: 8px; }
svg { flex: none; margin-top: var(--row); }
path { fill: none; stroke: var(--line); stroke-width: 1.2; opacity: 0.6; }
path.hl { outline: none; stroke: var(--hl); stroke-width: 2.5; opacity: 1; }
"#;

const SCRIPT: &str = r#"const mark = (selector) => document.querySelectorAll(selector).forEach((el) => el.classList.add("hl"));
document.addEventListener("mouseover", (event) => {
  document.querySelectorAll(".hl").forEach((el) => el.classList.remove("hl"));
  const target = event.target.closest("[data-in],[data-out]");
  if (!target) return;
  target.classList.add("hl");
  const { in: input, out: output } = target.dataset;
  const links = input !== undefined ? `path[data-in="${input}"]` : `path[data-out="${output}"]`;
  document.querySelectorAll(links).forEach((link) => {
    link.classList.add("hl");
    mark(`.row[data-in="${link.dataset.in}"]`);
    mark(`.row[data-out="${link.dataset.out}"]`);
  });
});
"#;

impl Report {
    pub fn html(&self) -> String {
        let height = self.originals.len().max(self.rows.len()) * ROW_HEIGHT;
        let center = |index: usize| index * ROW_HEIGHT + ROW_HEIGHT / 2;

        let mut body = String::from("<div class=\"map mono\">\n<div class=\"col\"><div class=\"head\">input</div>\n");
        for (index, text) in self.originals.iter().enumerate() {
            let class = if self.survived[index] { "row" } else { "row removed" };
            body.push_str(&format!("<div class=\"{class}\" data-in=\"{index}\">{}</div>\n", escape(text)));
        }
        body.push_str("</div>\n");

        body.push_str(&format!("<svg width=\"{LINK_WIDTH}\" height=\"{height}\" viewBox=\"0 0 {LINK_WIDTH} {height}\">\n"));
        for (index, row) in self.rows.iter().enumerate() {
            if let Some(origin) = row.origin_index {
                let (from, to, mid) = (center(origin), center(index), LINK_WIDTH / 2);
                body.push_str(&format!("<path data-in=\"{origin}\" data-out=\"{index}\" d=\"M0,{from} C{mid},{from} {mid},{to} {LINK_WIDTH},{to}\"/>\n"));
            }
        }
        body.push_str("</svg>\n");

        body.push_str("<div class=\"col\"><div class=\"head\">optimized</div>\n");
        for (index, row) in self.rows.iter().enumerate() {
            let class = match row.status {
                Status::Unchanged => "row",
                Status::Changed => "row changed",
                Status::New => "row new",
            };
            body.push_str(&format!(
                "<div class=\"{class}\" data-out=\"{index}\"><span class=\"block\">{}</span>{}</div>\n",
                escape(&row.block),
                escape(&row.text)
            ));
        }
        body.push_str("</div>\n</div>\n");

        let legend = [("removed", "no longer present"), ("changed", "changed"), ("new", "new, no provenance available")];
        html::page(&format!("Optimization provenance of {}", self.name), &legend, CSS, &body, SCRIPT)
    }

    pub fn count(&self, status: Status) -> usize {
        self.rows.iter().filter(|row| row.status == status).count()
    }
}
