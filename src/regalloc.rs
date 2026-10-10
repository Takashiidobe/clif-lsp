use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::flowgraph::ControlFlowGraph;
use cranelift_codegen::ir::{Block, Inst, RelSourceLoc, SourceLoc, Value, ValueDef, ValueLabel, ValueLabelAssignments, ValueLabelStart};
use cranelift_codegen::{Context, LabelValueLoc};

use crate::html::{self, escape};
use crate::stages::{self, Target};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    Def,
    Move,
    Reload,
    Spill,
}

impl Event {
    pub fn label(self) -> &'static str {
        match self {
            Event::Def => "def",
            Event::Move => "move",
            Event::Reload => "reload",
            Event::Spill => "spill",
        }
    }

}

pub struct Segment {
    pub value: String,
    pub place: String,
    pub event: Event,
    pub in_register: bool,
    pub from: usize,
    pub to: usize,
}

pub struct BlockInfo {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub insts: Vec<usize>,
    pub successors: Vec<usize>,
}

pub struct Report {
    pub name: String,
    pub blocks: Vec<BlockInfo>,
    pub insts: Vec<String>,
    pub segments: Vec<Segment>,
    pub spill_slots: usize,
    pub edits: usize,
}

pub fn analyze(target: &Target) -> Result<Report, String> {
    let isa = &*target.isa;
    let mut context = Context::for_function(target.function.clone());
    context.optimize(isa, &mut ControlPlane::default()).map_err(|e| stages::describe(&e))?;

    let blocks: Vec<Block> = context.func.layout.blocks().collect();
    let insts: Vec<Inst> = blocks.iter().flat_map(|&block| context.func.layout.block_insts(block)).collect();

    context.func.collect_debug_info();
    for (index, &inst) in insts.iter().enumerate() {
        context.func.set_srcloc(inst, SourceLoc::new(index as u32 + 1));
    }
    let mut labelled: Vec<(Value, RelSourceLoc)> = Vec::new();
    for &block in &blocks {
        labelled.extend(context.func.dfg.block_params(block).iter().map(|&param| (param, RelSourceLoc::default())));
    }
    for &inst in &insts {
        let from = context.func.stencil.srclocs[inst];
        labelled.extend(context.func.dfg.inst_results(inst).iter().map(|&result| (result, from)));
    }
    if let Some(labels) = context.func.dfg.values_labels.as_mut() {
        for (value, from) in labelled {
            let label = ValueLabel::from_u32(value.as_u32());
            labels.insert(value, ValueLabelAssignments::Starts(vec![ValueLabelStart { from, label }]));
        }
    }

    let mut regalloc = regalloc2::Ctx::default();
    let stencil = isa
        .compile_function(&context.func, &context.domtree, &mut regalloc, false, &mut ControlPlane::default())
        .map_err(|e| stages::describe(&e))?;
    let compiled = stencil.apply_params(&context.func.params);

    let spans: Vec<(u32, usize)> = compiled
        .buffer
        .get_srclocs_sorted()
        .iter()
        .filter(|s| !s.loc.is_default())
        .map(|s| (s.start, s.loc.bits() as usize - 1))
        .collect();
    let inst_at = |offset: u32| {
        let after = spans.partition_point(|&(start, _)| start <= offset);
        spans.get(after.saturating_sub(1)).map_or(0, |&(_, inst)| inst)
    };

    let mut segments: Vec<Segment> = Vec::new();
    let mut ranges: Vec<_> = compiled.value_labels_ranges.iter().collect();
    ranges.sort_by_key(|(label, _)| label.as_u32());
    for (label, locations) in ranges {
        let value = Value::from_u32(label.as_u32());
        let size = context.func.dfg.value_type(value).bytes().min(8) as u8;
        let mut locations = locations.clone();
        locations.sort_by_key(|range| range.start);
        let mut previous: Option<(usize, bool)> = None;
        for range in locations {
            let on_stack = matches!(range.loc, LabelValueLoc::CFAOffset(_));
            let place = match range.loc {
                LabelValueLoc::Reg(reg) => isa.pretty_print_reg(reg, size),
                LabelValueLoc::CFAOffset(offset) => format!("[cfa{offset:+}]"),
            };
            let mut from = inst_at(range.start);
            let to = inst_at(range.end.saturating_sub(1)).max(from);
            if let (None, ValueDef::Result(inst, _)) = (previous, context.func.dfg.value_def(value)) {
                from = insts.iter().position(|&i| i == inst).unwrap_or(from);
            }
            if let Some((index, _)) = previous.filter(|&(index, _)| segments[index].place == place) {
                segments[index].to = segments[index].to.max(to);
                continue;
            }
            let event = match previous {
                None => Event::Def,
                Some((_, false)) if on_stack => Event::Spill,
                Some((_, true)) if !on_stack => Event::Reload,
                Some(_) => Event::Move,
            };
            segments.push(Segment { value: value.to_string(), place, event, in_register: !on_stack, from, to });
            previous = Some((segments.len() - 1, on_stack));
        }
    }

    let cfg = ControlFlowGraph::with_function(&context.func);
    let mut next_inst = 0;
    let infos = blocks
        .iter()
        .map(|&block| {
            let count = context.func.layout.block_insts(block).count();
            let range = next_inst..next_inst + count;
            next_inst += count;
            let mut successors: Vec<usize> = cfg.succ_iter(block).filter_map(|succ| blocks.iter().position(|&b| b == succ)).collect();
            successors.dedup();
            BlockInfo {
                name: block.to_string(),
                params: context
                    .func
                    .dfg
                    .block_params(block)
                    .iter()
                    .map(|&param| (param.to_string(), format!("{param}: {}", context.func.dfg.value_type(param))))
                    .collect(),
                insts: range.collect(),
                successors,
            }
        })
        .collect();

    Ok(Report {
        name: target.name.clone(),
        blocks: infos,
        insts: insts.iter().map(|&inst| context.func.dfg.display_inst(inst).to_string()).collect(),
        segments,
        spill_slots: regalloc.output.num_spillslots,
        edits: regalloc.output.edits.len(),
    })
}

const CSS: &str = r#"
.scroll { overflow: auto; max-height: calc(100vh - 110px); }
table { border-collapse: collapse; }
th, td { border: 1px solid var(--border); padding: 0 8px; height: var(--row); white-space: pre; text-align: center; }
th { background: var(--head); position: sticky; top: 0; z-index: 2; }
th:first-child, td:first-child { position: sticky; left: 0; text-align: left; background: var(--bg); z-index: 1; }
th:first-child { z-index: 3; background: var(--head); }
tr.block td { background: var(--head); font-weight: 600; text-align: left; }
tr.block td span { color: var(--muted); font-weight: 400; margin-left: 12px; }
td.reg { background: var(--reg); }
td.stack { background: var(--stack); }
td.spill { background: var(--spill); }
td.reload { background: var(--reload); }
td.move { background: var(--move); }
td.pressure { text-align: left; min-width: 160px; }
td.pressure i { display: inline-block; height: 10px; background: var(--hl); opacity: 0.55; margin-right: 8px; vertical-align: -1px; }
td.pressure.peak b { font-weight: 700; }
"#;

impl Report {
    pub fn is_param(&self, value: &str) -> bool {
        self.blocks.iter().any(|block| block.params.iter().any(|(name, _)| name == value))
    }

    pub fn count(&self, event: Event) -> usize {
        self.segments.iter().filter(|s| s.event == event).count()
    }

    fn covering(&self, value: &str, inst: usize) -> Option<&Segment> {
        self.segments.iter().rev().find(|s| s.value == value && s.from <= inst && inst <= s.to)
    }

    pub fn in_registers(&self, inst: usize) -> Vec<&str> {
        let mut values: Vec<&str> = Vec::new();
        for segment in self.segments.iter().filter(|s| s.in_register && s.from <= inst && inst <= s.to) {
            if !values.contains(&segment.value.as_str()) {
                values.push(&segment.value);
            }
        }
        values
    }

    pub fn max_pressure(&self) -> usize {
        (0..self.insts.len()).map(|inst| self.in_registers(inst).len()).max().unwrap_or(0)
    }

    fn placed_values(&self) -> Vec<&str> {
        let mut values: Vec<&str> = Vec::new();
        for segment in &self.segments {
            if !values.contains(&segment.value.as_str()) {
                values.push(&segment.value);
            }
        }
        values
    }

    pub fn html(&self) -> String {
        let values = self.placed_values();
        let peak = self.max_pressure();
        let width = values.len() + 2;
        let mut body = String::from("<div class=\"scroll mono\"><table>\n<tr><th>instruction</th>");
        for value in &values {
            body.push_str(&format!("<th>{}</th>", escape(value)));
        }
        body.push_str("<th>registers live</th></tr>\n");
        for block in &self.blocks {
            let successors: Vec<&str> = block.successors.iter().map(|&target| self.blocks[target].name.as_str()).collect();
            let edges = if successors.is_empty() { String::new() } else { format!("<span>&rarr; {}</span>", escape(&successors.join(", "))) };
            body.push_str(&format!("<tr class=\"block\"><td colspan=\"{width}\">{}{edges}</td></tr>\n", escape(&block.name)));
            for &inst in &block.insts {
                body.push_str(&format!("<tr><td>{}</td>", escape(&self.insts[inst])));
                for value in &values {
                    match self.covering(value, inst) {
                        None => body.push_str("<td></td>"),
                        Some(segment) => {
                            let starts = segment.from == inst;
                            let class = match (starts, segment.event, segment.in_register) {
                                (true, Event::Def, _) | (false, _, true) => "reg",
                                (true, Event::Spill, _) => "spill",
                                (true, Event::Reload, _) => "reload",
                                (true, Event::Move, _) => "move",
                                (false, _, false) => "stack",
                            };
                            let text = if starts { escape(&segment.place) } else { "|".to_owned() };
                            body.push_str(&format!("<td class=\"{class}\">{text}</td>"));
                        }
                    }
                }
                let live = self.in_registers(inst).len();
                let (class, count) = if live == peak && peak > 0 { ("pressure peak", format!("<b>{live}</b>")) } else { ("pressure", live.to_string()) };
                body.push_str(&format!("<td class=\"{class}\"><i style=\"width:{}px\"></i>{count}</td></tr>\n", live * 10));
            }
        }
        body.push_str("</table></div>\n");
        let legend = [("reg", "in a register"), ("stack", "on the stack"), ("spill", "spill"), ("reload", "reload"), ("move", "move")];
        html::page(&format!("Register allocation of {}", self.name), &legend, CSS, &body, "")
    }
}
