use cranelift_codegen::{CodegenError, Context};
use cranelift_codegen::ir::Function;
use cranelift_codegen::settings::{self, Configurable, OptLevel};
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

pub fn describe(error: &CodegenError) -> String {
    match error {
        CodegenError::Verifier(errors) => format!("verifier errors:\n{}", errors.to_string().trim_end()),
        other => other.to_string(),
    }
}

pub fn compile(function: &Function, isa: &dyn TargetIsa) -> Result<Compiled, String> {
    let mut context = Context::for_function(function.clone());
    context.set_disasm(true);
    let compiled = context.compile(isa, &mut ControlPlane::default()).map_err(|e| describe(&e.inner))?;
    let vcode = compiled.vcode.clone().ok_or("the backend produced no vcode")?;
    Ok(Compiled { vcode, code_bytes: compiled.code_buffer().len() })
}

pub struct Snapshot {
    pub name: &'static str,
    pub text: String,
    pub skipped: Option<&'static str>,
    pub ir: bool,
}

pub fn snapshots(target: &Target) -> Result<Vec<Snapshot>, String> {
    let isa = &*target.isa;
    let mut plane = ControlPlane::default();
    let mut context = Context::for_function(target.function.clone());
    let mut out = vec![Snapshot { name: "parsed", text: context.func.to_string(), skipped: None, ir: true }];

    let canonicalize = isa.flags().enable_nan_canonicalization();
    if canonicalize {
        context.canonicalize_nans(isa).map_err(|e| describe(&e))?;
    }
    out.push(Snapshot { name: "canonicalize_nans", text: context.func.to_string(), skipped: (!canonicalize).then_some("enable_nan_canonicalization is off"), ir: true });

    context.verify_if(isa).map_err(|e| describe(&e))?;
    context.compute_cfg();
    context.compute_domtree();
    context.eliminate_unreachable_code(isa).map_err(|e| describe(&e))?;
    out.push(Snapshot { name: "unreachable_code", text: context.func.to_string(), skipped: None, ir: true });

    context.remove_constant_phis(isa).map_err(|e| describe(&e))?;
    context.func.dfg.resolve_all_aliases();
    out.push(Snapshot { name: "constant_phis", text: context.func.to_string(), skipped: None, ir: true });

    let optimize = isa.flags().opt_level() != OptLevel::None;
    if optimize {
        context.egraph_pass(isa, &mut plane).map_err(|e| describe(&e))?;
    }
    out.push(Snapshot { name: "egraph", text: context.func.to_string(), skipped: (!optimize).then_some("opt_level=none"), ir: true });

    out.push(Snapshot { name: "vcode", text: compile(&target.function, isa)?.vcode, skipped: None, ir: false });
    Ok(out)
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
            context.optimize(&*isa, &mut ControlPlane::default()).map_err(|e| describe(&e))?;
            context.func.to_string()
        }
        Stage::Vcode => compile(&ir, &*isa)?.vcode,
    };
    Ok(Rendered { function: name, text })
}
