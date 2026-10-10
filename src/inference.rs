use std::collections::HashMap;

use cranelift_reader::{ParseOptions, parse_test};

use crate::document::Document;
use crate::syntax::Item;

pub fn value_types(document: &Document) -> HashMap<usize, String> {
    let mut out = HashMap::new();
    let Ok(test) = parse_test(document.text(), ParseOptions::default()) else { return out };
    let functions = document.file().items.iter().filter_map(|item| match item {
        Item::Function(function) => Some(function),
        Item::Directive(_) => None,
    });
    for ((ir, _), function) in test.functions.iter().zip(functions) {
        for (block_index, block) in ir.layout.blocks().enumerate() {
            let Some(source_block) = function.blocks.get(block_index) else { continue };
            for (param, value) in source_block.params.iter().zip(ir.dfg.block_params(block)) {
                out.insert(param.value.start, ir.dfg.value_type(*value).to_string());
            }
            for (inst_index, inst) in ir.layout.block_insts(block).enumerate() {
                let Some(source_inst) = source_block.insts.get(inst_index) else { continue };
                for (result, value) in source_inst.results.iter().zip(ir.dfg.inst_results(inst)) {
                    out.insert(result.start, ir.dfg.value_type(*value).to_string());
                }
            }
        }
    }
    out
}
