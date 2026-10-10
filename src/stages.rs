use cranelift_codegen::Context;
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::isa::{self, OwnedTargetIsa};
use cranelift_reader::{IsaSpec, ParseOptions, parse_test};

use crate::document::Document;
use crate::syntax::Item;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Parsed,
    Optimized,
    Vcode,
}

impl Stage {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "parsed" => Some(Stage::Parsed),
            "optimized" => Some(Stage::Optimized),
            "vcode" => Some(Stage::Vcode),
            _ => None,
        }
    }
}

pub struct Rendered {
    pub function: String,
    pub text: String,
}

fn isa_of(spec: &IsaSpec) -> Result<OwnedTargetIsa, String> {
    match spec {
        IsaSpec::Some(isas) => Ok(isas[0].clone()),
        IsaSpec::None(flags) => isa::lookup_by_name("x86_64")
            .map_err(|e| e.to_string())?
            .finish(flags.clone())
            .map_err(|e| e.to_string()),
    }
}

pub fn render(document: &Document, function: Option<&str>, stage: Stage) -> Result<Rendered, String> {
    let test = parse_test(document.text(), ParseOptions::default()).map_err(|e| format!("line {}: {}", e.location.line_number, e.message))?;
    let names: Vec<String> = document
        .file()
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f.name.map_or_else(String::new, |n| n.text(document.text()).to_owned())),
            Item::Directive(_) => None,
        })
        .collect();
    let index = match function {
        Some(wanted) => names.iter().position(|n| n == wanted).ok_or_else(|| format!("no function `{wanted}`"))?,
        None => 0,
    };
    let (ir, _) = test.functions.get(index).ok_or("the file has no functions")?;
    let name = names.get(index).cloned().unwrap_or_default();

    let text = match stage {
        Stage::Parsed => ir.to_string(),
        Stage::Optimized => {
            let mut context = Context::for_function(ir.clone());
            context.optimize(&*isa_of(&test.isa_spec)?, &mut ControlPlane::default()).map_err(|e| format!("{e:?}"))?;
            context.func.to_string()
        }
        Stage::Vcode => {
            let isa = isa_of(&test.isa_spec)?;
            let mut context = Context::for_function(ir.clone());
            context.set_disasm(true);
            let compiled = context.compile(&*isa, &mut ControlPlane::default()).map_err(|e| format!("{:?}", e.inner))?;
            compiled.vcode.clone().ok_or("the backend produced no vcode")?
        }
    };
    Ok(Rendered { function: name, text })
}
