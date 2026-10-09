use std::panic::{AssertUnwindSafe, catch_unwind};

use clif_lsp::lexer::lex;
use clif_lsp::parser::parse;
use clif_lsp::syntax::{File, Item, Span};

fn all_spans(file: &File) -> Vec<Span> {
    let mut spans: Vec<Span> = file.comments.clone();
    spans.extend(file.errors.iter().map(|e| e.span));
    for item in &file.items {
        match item {
            Item::Directive(d) => {
                spans.push(d.span);
                spans.push(d.keyword);
                spans.extend(d.args.iter().map(|&t| Span::from(t)));
            }
            Item::Function(f) => {
                spans.push(f.span);
                spans.extend(f.name);
                spans.extend(&f.signature.params);
                spans.extend(&f.signature.returns);
                spans.extend(f.signature.call_conv);
                for decl in &f.decls {
                    spans.push(decl.span);
                    spans.push(decl.name.into());
                    spans.extend(decl.rhs.iter().map(|&t| Span::from(t)));
                }
                for block in &f.blocks {
                    spans.push(block.span);
                    spans.push(block.name.into());
                    for param in &block.params {
                        spans.push(param.span);
                        spans.push(param.value.into());
                        spans.extend(param.ty);
                    }
                    for inst in &block.insts {
                        spans.push(inst.span);
                        spans.extend(inst.results.iter().map(|&t| Span::from(t)));
                        spans.extend(inst.opcode);
                        spans.extend(&inst.type_suffix);
                        spans.extend(inst.operands.iter().map(|&t| Span::from(t)));
                    }
                }
            }
        }
    }
    spans
}

fn check_one(source: &str) -> Result<(), String> {
    let mut expected_start = 0;
    for token in lex(source) {
        if token.start != expected_start || token.end <= token.start {
            return Err(format!("lexer gap, overlap or empty token at byte {}", token.start));
        }
        expected_start = token.end;
    }
    if expected_start != source.len() {
        return Err(format!("lexer stopped at byte {expected_start} of {}", source.len()));
    }

    let file = catch_unwind(AssertUnwindSafe(|| parse(source))).map_err(|_| "parser panicked".to_owned())?;
    for span in all_spans(&file) {
        let ok = span.start <= span.end
            && span.end <= source.len()
            && source.is_char_boundary(span.start)
            && source.is_char_boundary(span.end);
        if !ok {
            return Err(format!("span {}..{} is out of bounds or splits a character", span.start, span.end));
        }
    }
    Ok(())
}

pub fn check(source: &str) -> Result<(), String> {
    check_one(source)?;
    for end in (0..source.len()).filter(|&i| source.is_char_boundary(i)) {
        check_one(&source[..end]).map_err(|e| format!("{e} (input truncated to {end} bytes)"))?;
    }
    Ok(())
}
