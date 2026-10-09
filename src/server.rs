use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, InitializeParams,
    InitializeResult, Diagnostic, DocumentSymbolParams, DocumentSymbolResponse, OneOf, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::diagnostics;
use crate::document::Document;
use crate::symbols;

pub struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Uri, Document>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self { client, documents: Arc::default() }
    }

    async fn publish(&self, uri: Uri, version: Option<i32>, diagnostics: Vec<Diagnostic>) {
        self.client.publish_diagnostics(uri, diagnostics, version).await;
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::INCREMENTAL)),
                document_symbol_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo { name: "clif-lsp".into(), version: Some(env!("CARGO_PKG_VERSION").into()) }),
            offset_encoding: None,
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        let document = Document::new(&doc.text);
        let found = diagnostics::all(&document);
        self.documents.write().await.insert(doc.uri.clone(), document);
        self.publish(doc.uri, Some(doc.version), found).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let found = {
            let mut documents = self.documents.write().await;
            let Some(document) = documents.get_mut(&uri) else { return };
            for change in params.content_changes {
                document.apply(change.range, &change.text);
            }
            diagnostics::all(document)
        };
        self.publish(uri, Some(params.text_document.version), found).await;
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        let documents = self.documents.read().await;
        let found = documents.get(&params.text_document.uri).map(symbols::document);
        Ok(found.map(DocumentSymbolResponse::Nested))
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        self.publish(uri, None, Vec::new()).await;
    }
}
