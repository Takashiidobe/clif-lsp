use cranelift_codegen::Context;
use cranelift_codegen::ir::Function;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::isa::{self, OwnedTargetIsa, TargetIsa};
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

pub struct Target {
    pub name: String,
    pub function: Function,
    pub isa: OwnedTargetIsa,
}

pub fn target(document: &Document, function: Option<&str>) -> Result<Target, String> {
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
    let (function, _) = test.functions.get(index).ok_or("the file has no functions")?;
    Ok(Target { name: names.get(index).cloned().unwrap_or_default(), function: function.clone(), isa: isa_of(&test.isa_spec)? })
}

pub struct Compiled {
    pub vcode: String,
    pub code_bytes: usize,
}

pub fn compile(function: &Function, isa: &dyn TargetIsa) -> Result<Compiled, String> {
    let mut context = Context::for_function(function.clone());
    context.set_disasm(true);
    let compiled = context.compile(isa, &mut ControlPlane::default()).map_err(|e| format!("{:?}", e.inner))?;
    let vcode = compiled.vcode.clone().ok_or("the backend produced no vcode")?;
    Ok(Compiled { vcode, code_bytes: compiled.code_buffer().len() })
}

pub fn with_algorithm(isa: &dyn TargetIsa, algorithm: &str) -> Result<OwnedTargetIsa, String> {
    let mut shared = settings::builder();
    for line in isa.flags().to_string().lines().skip(1) {
        let Some((name, value)) = line.split_once(" = ") else { continue };
        let value = if name == "regalloc_algorithm" { algorithm } else { value.trim_matches('"') };
        shared.set(name, value).map_err(|e| e.to_string())?;
    }
    let mut builder = isa::lookup(isa.triple().clone()).map_err(|e| e.to_string())?;
    for value in isa.isa_flags() {
        builder.set(value.name, &value.value_string()).map_err(|e| e.to_string())?;
    }
    builder.finish(settings::Flags::new(shared)).map_err(|e| e.to_string())
}

pub fn render(document: &Document, function: Option<&str>, stage: Stage) -> Result<Rendered, String> {
    let Target { name, function: ir, isa } = target(document, function)?;
    let text = match stage {
        Stage::Parsed => ir.to_string(),
        Stage::Optimized => {
            let mut context = Context::for_function(ir);
            context.optimize(&*isa, &mut ControlPlane::default()).map_err(|e| format!("{e:?}"))?;
            context.func.to_string()
        }
        Stage::Vcode => compile(&ir, &*isa)?.vcode,
    };
    Ok(Rendered { function: name, text })
}
