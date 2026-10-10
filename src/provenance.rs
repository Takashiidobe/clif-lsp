use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::ir::{Function, Inst, SourceLoc};
use cranelift_codegen::Context;

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
}

pub struct Report {
    pub name: String,
    pub inputs: usize,
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
            let origin = (!loc.is_default()).then(|| loc.bits() as usize - 1).filter(|&index| index < originals.len());
            let (status, origin) = match origin {
                None => (Status::New, None),
                Some(index) => {
                    survived[index] = true;
                    let status = if originals[index] == text { Status::Unchanged } else { Status::Changed };
                    (status, Some(originals[index].clone()))
                }
            };
            rows.push(Row { block: block.to_string(), text, status, origin });
        }
    }
    let removed = originals.iter().zip(&survived).filter(|&(_, &kept)| !kept).map(|(text, _)| text.clone()).collect();
    Ok(Report { name: target.name.clone(), inputs: originals.len(), rows, removed })
}

impl Report {
    pub fn count(&self, status: Status) -> usize {
        self.rows.iter().filter(|row| row.status == status).count()
    }
}
