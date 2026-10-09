use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, Range};

use crate::document::Document;

const SOURCE: &str = "clif-lsp";

pub fn syntax(document: &Document) -> Vec<Diagnostic> {
    document
        .file()
        .errors
        .iter()
        .map(|error| Diagnostic {
            range: Range::new(document.position(error.span.start), document.position(error.span.end)),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some(SOURCE.to_owned()),
            message: error.message.clone(),
            ..Diagnostic::default()
        })
        .collect()
}
