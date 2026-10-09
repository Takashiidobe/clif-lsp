use clif_lsp::parser::parse;
use clif_lsp::syntax::Item;

pub fn check(relative_path: &str, source: &str) -> Result<(), String> {
    let ours = parse(source);
    let reference = cranelift_reader::parse_functions(source);
    let strict = relative_path.starts_with("valid/");

    match reference {
        Err(error) if ours.errors.is_empty() && strict => {
            Err(format!("cranelift-reader rejects this file but we report no errors: {error}"))
        }
        Err(_) => Ok(()),
        Ok(_) if !ours.errors.is_empty() => Err(format!(
            "cranelift-reader accepts this file but we report: {}",
            ours.errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("; ")
        )),
        Ok(functions) if strict => {
            let mut counts = (0, 0, 0);
            for item in &ours.items {
                if let Item::Function(f) = item {
                    counts.0 += 1;
                    counts.1 += f.blocks.len();
                    counts.2 += f.blocks.iter().map(|b| b.insts.len()).sum::<usize>();
                }
            }
            let mut expected = (0, 0, 0);
            for function in &functions {
                expected.0 += 1;
                for block in function.layout.blocks() {
                    expected.1 += 1;
                    expected.2 += function.layout.block_insts(block).count();
                }
            }
            if counts == expected {
                Ok(())
            } else {
                Err(format!(
                    "(functions, blocks, insts) differ: ours {counts:?}, cranelift-reader {expected:?}"
                ))
            }
        }
        Ok(_) => Ok(()),
    }
}
