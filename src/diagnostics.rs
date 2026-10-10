use cranelift_codegen::ir::entities::AnyEntity;
use cranelift_codegen::ir::{Block, Function as IrFunction, Inst};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::settings::FlagsOrIsa;
use cranelift_codegen::verifier::verify_function;
use cranelift_reader::{ParseOptions, parse_test};
use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, Position, Range};

use crate::cfg;
use crate::document::Document;
use crate::resolve::{self, NameKind, Resolution};
use crate::syntax::{Function, Item, Span, SyntaxError};

const SOURCE: &str = "clif-lsp";

pub fn all(document: &Document) -> Vec<Diagnostic> {
    let file = document.file();
    let resolution = resolve::resolve(file, document.text());
    let syntax_clean = file.errors.is_empty();

    let mut out: Vec<Diagnostic> = file.errors.iter().map(|e| at_span(document, e, DiagnosticSeverity::ERROR)).collect();
    let severity = if syntax_clean { DiagnosticSeverity::ERROR } else { DiagnosticSeverity::WARNING };
    out.extend(resolution.problems.iter().map(|p| at_span(document, p, severity)));
    if syntax_clean {
        out.extend(unreachable_blocks(document));
    }
    if syntax_clean && resolution.problems.is_empty() {
        out.extend(semantic(document, &resolution));
    }
    out
}

fn unreachable_blocks(document: &Document) -> Vec<Diagnostic> {
    cfg::graphs(document)
        .into_iter()
        .flat_map(|graph| graph.blocks)
        .filter(|block| !block.reachable)
        .map(|block| {
            let range = Range::new(document.position(block.span.start), document.position(block.span.end));
            Diagnostic {
                tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                ..diagnostic(range, format!("block `{}` is unreachable", block.name), DiagnosticSeverity::HINT)
            }
        })
        .collect()
}

fn at_span(document: &Document, error: &SyntaxError, severity: DiagnosticSeverity) -> Diagnostic {
    let range = Range::new(document.position(error.span.start), document.position(error.span.end));
    diagnostic(range, error.message.clone(), severity)
}

fn diagnostic(range: Range, message: String, severity: DiagnosticSeverity) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(severity),
        source: Some(SOURCE.to_owned()),
        message,
        ..Diagnostic::default()
    }
}

fn semantic(document: &Document, resolution: &Resolution) -> Vec<Diagnostic> {
    let text = document.text();
    let test = match parse_test(text, ParseOptions::default()) {
        Ok(test) => test,
        Err(error) => {
            let range = reader_error_range(document, error.location.line_number, &error.message);
            return vec![diagnostic(range, error.message, DiagnosticSeverity::ERROR)];
        }
    };

    let functions: Vec<(usize, &Function)> = document
        .file()
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            Item::Function(f) => Some((index, f)),
            Item::Directive(_) => None,
        })
        .collect();

    let mut out = Vec::new();
    for (position, (ir, _)) in test.functions.iter().enumerate() {
        let Some(&(scope, function)) = functions.get(position) else { break };
        let Err(errors) = verify_function(ir, flags_or_isa(&test.isa_spec)) else { continue };
        for error in errors.0 {
            let span = locate(ir, function, scope, resolution, error.location);
            let range = Range::new(document.position(span.start), document.position(span.end));
            out.push(diagnostic(range, error.message, DiagnosticSeverity::ERROR));
        }
    }
    out
}

fn reader_error_range(document: &Document, line_number: usize, message: &str) -> Range {
    let line = line_number.saturating_sub(1) as u32;
    let line_start = document.offset(Position::new(line, 0));
    if reports_lookahead(document.text(), line_start, message)
        && let Some(span) = statement_before(document, line_start)
    {
        return Range::new(document.position(span.start), document.position(span.end));
    }
    line_range(document, line)
}

fn reports_lookahead(text: &str, line_start: usize, message: &str) -> bool {
    let line_end = text[line_start..].find('\n').map_or(text.len(), |n| line_start + n);
    let blanked = format!("{}{}{}", &text[..line_start], " ".repeat(line_end - line_start), &text[line_end..]);
    parse_test(&blanked, ParseOptions::default())
        .err()
        .is_some_and(|retry| retry.message == message)
}

fn statement_before(document: &Document, offset: usize) -> Option<Span> {
    document
        .file()
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f),
            Item::Directive(_) => None,
        })
        .flat_map(|f| {
            let decls = f.decls.iter().map(|d| d.span);
            let blocks = f.blocks.iter().flat_map(|b| std::iter::once(b.span).chain(b.insts.iter().map(|i| i.span)));
            decls.chain(blocks)
        })
        .filter(|span| span.end <= offset)
        .max_by_key(|span| span.end)
}

fn flags_or_isa(spec: &cranelift_reader::IsaSpec) -> FlagsOrIsa<'_> {
    match spec {
        cranelift_reader::IsaSpec::None(flags) => FlagsOrIsa { flags, isa: None },
        cranelift_reader::IsaSpec::Some(isas) => {
            let isa: &dyn TargetIsa = &*isas[0];
            FlagsOrIsa { flags: isa.flags(), isa: Some(isa) }
        }
    }
}

fn locate(ir: &IrFunction, function: &Function, scope: usize, resolution: &Resolution, location: AnyEntity) -> Span {
    let precise = match location {
        AnyEntity::Inst(inst) => inst_span(ir, function, inst),
        AnyEntity::Block(block) => block_span(ir, function, block),
        AnyEntity::Function => None,
        entity => {
            let name = entity.to_string();
            resolution
                .defs
                .iter()
                .find(|d| d.scope == Some(scope) && d.kind != NameKind::Block && d.name == name)
                .map(|d| d.span)
        }
    };
    precise.or(function.name).unwrap_or(function.span)
}

fn block_span(ir: &IrFunction, function: &Function, block: Block) -> Option<Span> {
    let index = ir.layout.blocks().position(|b| b == block)?;
    function.blocks.get(index).map(|b| b.name.into())
}

fn inst_span(ir: &IrFunction, function: &Function, inst: Inst) -> Option<Span> {
    for (index, block) in ir.layout.blocks().enumerate() {
        if let Some(at) = ir.layout.block_insts(block).position(|i| i == inst) {
            return function.blocks.get(index)?.insts.get(at).map(|i| i.span);
        }
    }
    None
}

fn line_range(document: &Document, line: u32) -> Range {
    let start = document.offset(Position::new(line, 0));
    let text = document.text();
    let end = text[start..].find('\n').map_or(text.len(), |n| start + n);
    let body = text[start..end].trim_end_matches('\r');
    let lead = body.len() - body.trim_start().len();
    Range::new(document.position(start + lead), document.position(start + body.trim_end().len()))
}
