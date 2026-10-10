use clif_lsp::document::Document;
use clif_lsp::formatting;
use clif_lsp::lexer::{TokenKind, lex};

fn format(source: &str) -> String {
    let mut document = Document::new(source);
    let edits = formatting::edits(&document, None);
    formatting::apply(&mut document, edits);
    document.text().to_owned()
}

fn significant(source: &str) -> Vec<String> {
    lex(source)
        .into_iter()
        .filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Newline))
        .map(|t| t.text(source).trim_end().to_owned())
        .collect()
}

fn check_one(source: &str) -> Result<(), String> {
    let formatted = format(source);
    if significant(source) != significant(&formatted) {
        return Err("formatting changed the tokens".to_owned());
    }
    if format(&formatted) != formatted {
        return Err("formatting is not idempotent".to_owned());
    }
    Ok(())
}

pub fn check(source: &str) -> Result<(), String> {
    check_one(source)?;
    let bytes = source.as_bytes();
    let cuts = (1..source.len()).filter(|&i| source.is_char_boundary(i) && matches!(bytes[i - 1], b'\n' | b' ' | b'{' | b','));
    for end in cuts {
        check_one(&source[..end]).map_err(|e| format!("{e} (input truncated to {end} bytes)"))?;
    }

    let Ok(before) = cranelift_reader::parse_functions(source) else { return Ok(()) };
    let after = cranelift_reader::parse_functions(&format(source))
        .map_err(|e| format!("cranelift-reader rejects the formatted output: {e}"))?;
    let print = |functions: &[cranelift_codegen::ir::Function]| functions.iter().map(ToString::to_string).collect::<Vec<_>>();
    if print(&before) != print(&after) {
        return Err("formatting changed what cranelift-reader sees".to_owned());
    }
    Ok(())
}
