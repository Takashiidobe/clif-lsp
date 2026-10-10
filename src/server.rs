use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::{Error, Result};
use tower_lsp_server::ls_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem, CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams, CallHierarchyServerCapability, DocumentHighlight, DocumentHighlightKind, DocumentFormattingParams, DocumentHighlightParams, DocumentRangeFormattingParams, FoldingRange, FoldingRangeParams, FoldingRangeProviderCapability, SelectionRange, SelectionRangeParams, SelectionRangeProviderCapability, SignatureHelp, SignatureHelpOptions, SignatureHelpParams, CodeActionOptions, CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CodeActionKind, CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, InitializeParams,
    InitializeResult, InlayHint, InlayHintParams, Diagnostic, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability, Location, OneOf,
    PrepareRenameResponse, ReferenceParams, RenameOptions, RenameParams, SemanticTokens, SemanticTokensOptions, SemanticTokensParams, SemanticTokensFullOptions, SemanticTokensResult,
    SemanticTokensServerCapabilities, TextDocumentPositionParams, TextEdit, WorkspaceEdit, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use serde::{Deserialize, Serialize};
use tower_lsp_server::{Client, ClientSocket, LanguageServer, LspService};
use tower_lsp_server::ls_types::TextDocumentIdentifier;

use crate::actions;
use crate::analysis;
use crate::calls;
use crate::cfg;
use crate::completion;
use crate::diagnostics;
use crate::document::Document;
use crate::formatting;
use crate::hover;
use crate::inlay;
use crate::navigation;
use crate::ranges;
use crate::semantic;
use crate::signature;
use crate::stages::{self, Stage};
use crate::symbols;
use crate::view;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Serialize)]
pub struct GraphResponse {
    pub functions: Vec<cfg::Graph>,
}

pub fn service() -> (LspService<Backend>, ClientSocket) {
    LspService::build(Backend::new).custom_method("clif/controlFlowGraph", Backend::control_flow_graph)
        .custom_method("clif/analysis", Backend::analysis)
        .custom_method("clif/stage", Backend::stage)
        .custom_method("clif/view", Backend::view).finish()
}

#[derive(Serialize)]
pub struct AnalysisResponse {
    pub functions: Vec<analysis::FunctionAnalysis>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageParams {
    pub text_document: TextDocumentIdentifier,
    pub function: Option<String>,
    pub stage: String,
}

#[derive(Serialize)]
pub struct StageResponse {
    pub function: String,
    pub stage: String,
    pub text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewParams {
    pub text_document: TextDocumentIdentifier,
    pub view: String,
    pub function: Option<String>,
    #[serde(default)]
    pub args: serde_json::Value,
}

pub struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Uri, Document>>>,
    snippets: AtomicBool,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self { client, documents: Arc::default(), snippets: AtomicBool::new(false) }
    }

    async fn control_flow_graph(&self, params: GraphParams) -> Result<GraphResponse> {
        let documents = self.documents.read().await;
        let document = documents.get(&params.text_document.uri).ok_or_else(|| Error::invalid_params("document is not open"))?;
        Ok(GraphResponse { functions: cfg::graphs(document) })
    }

    async fn analysis(&self, params: GraphParams) -> Result<AnalysisResponse> {
        let documents = self.documents.read().await;
        let document = documents.get(&params.text_document.uri).ok_or_else(|| Error::invalid_params("document is not open"))?;
        Ok(AnalysisResponse { functions: analysis::analyze(document) })
    }

    async fn stage(&self, params: StageParams) -> Result<StageResponse> {
        let stage = Stage::parse(&params.stage).ok_or_else(|| Error::invalid_params(format!("unknown stage `{}`", params.stage)))?;
        let documents = self.documents.read().await;
        let document = documents.get(&params.text_document.uri).ok_or_else(|| Error::invalid_params("document is not open"))?;
        let rendered = stages::render(document, params.function.as_deref(), stage).map_err(Error::invalid_params)?;
        Ok(StageResponse { function: rendered.function, stage: params.stage, text: rendered.text })
    }

    async fn view(&self, params: ViewParams) -> Result<view::View> {
        let documents = self.documents.read().await;
        let document = documents.get(&params.text_document.uri).ok_or_else(|| Error::invalid_params("document is not open"))?;
        view::render(document, &params.view, params.function.as_deref(), &params.args).map_err(Error::invalid_params)
    }

    async fn publish(&self, uri: Uri, version: Option<i32>, diagnostics: Vec<Diagnostic>) {
        self.client.publish_diagnostics(uri, diagnostics, version).await;
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let snippets = params
            .capabilities
            .text_document
            .and_then(|t| t.completion)
            .and_then(|c| c.completion_item)
            .and_then(|i| i.snippet_support)
            .unwrap_or(false);
        self.snippets.store(snippets, Ordering::Relaxed);
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::INCREMENTAL)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into(), "%".into()]),
                    ..CompletionOptions::default()
                }),
                semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
                    SemanticTokensOptions {
                        legend: semantic::legend(),
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        ..SemanticTokensOptions::default()
                    },
                )),
                code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
                    code_action_kinds: Some(vec![CodeActionKind::QUICKFIX, CodeActionKind::REFACTOR_REWRITE]),
                    ..CodeActionOptions::default()
                })),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![" ".into(), ",".into(), "(".into()]),
                    retrigger_characters: Some(vec![",".into()]),
                    work_done_progress_options: Default::default(),
                }),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
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

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let at = params.text_document_position;
        let documents = self.documents.read().await;
        let snippets = self.snippets.load(Ordering::Relaxed);
        let items = documents.get(&at.text_document.uri).map(|d| completion::at(d, at.position, snippets));
        Ok(items.map(CompletionResponse::Array))
    }

    async fn semantic_tokens_full(&self, params: SemanticTokensParams) -> Result<Option<SemanticTokensResult>> {
        let documents = self.documents.read().await;
        let data = documents.get(&params.text_document.uri).map(semantic::tokens);
        Ok(data.map(|data| SemanticTokensResult::Tokens(SemanticTokens { result_id: None, data })))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let documents = self.documents.read().await;
        let found = documents.get(&params.text_document.uri).map(|d| actions::at(d, &params.text_document.uri, params.range));
        Ok(found)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let at = params.text_document_position_params;
        let documents = self.documents.read().await;
        Ok(documents.get(&at.text_document.uri).and_then(|d| signature::at(d, at.position)))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let documents = self.documents.read().await;
        Ok(documents.get(&params.text_document.uri).map(ranges::folding))
    }

    async fn selection_range(&self, params: SelectionRangeParams) -> Result<Option<Vec<SelectionRange>>> {
        let documents = self.documents.read().await;
        let found = documents.get(&params.text_document.uri);
        Ok(found.map(|d| params.positions.iter().map(|p| ranges::selection(d, *p)).collect()))
    }

    async fn document_highlight(&self, params: DocumentHighlightParams) -> Result<Option<Vec<DocumentHighlight>>> {
        let at = params.text_document_position_params;
        let documents = self.documents.read().await;
        let Some(document) = documents.get(&at.text_document.uri) else { return Ok(None) };
        let found = navigation::highlights(document, at.position).into_iter().map(|(range, definition)| DocumentHighlight {
            range,
            kind: Some(if definition { DocumentHighlightKind::WRITE } else { DocumentHighlightKind::READ }),
        });
        Ok(Some(found.collect()))
    }

    async fn prepare_call_hierarchy(&self, params: CallHierarchyPrepareParams) -> Result<Option<Vec<CallHierarchyItem>>> {
        let at = params.text_document_position_params;
        let documents = self.documents.read().await;
        let found = documents.get(&at.text_document.uri).map(|d| calls::prepare(d, &at.text_document.uri, at.position));
        Ok(found)
    }

    async fn incoming_calls(&self, params: CallHierarchyIncomingCallsParams) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        let documents = self.documents.read().await;
        let found = documents.get(&params.item.uri).map(|d| calls::incoming(d, &params.item.uri, &params.item));
        Ok(found)
    }

    async fn outgoing_calls(&self, params: CallHierarchyOutgoingCallsParams) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        let documents = self.documents.read().await;
        let found = documents.get(&params.item.uri).map(|d| calls::outgoing(d, &params.item.uri, &params.item));
        Ok(found)
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let documents = self.documents.read().await;
        Ok(documents.get(&params.text_document.uri).map(|d| inlay::within(d, params.range)))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let documents = self.documents.read().await;
        Ok(documents.get(&params.text_document.uri).map(|d| formatting::edits(d, None)))
    }

    async fn range_formatting(&self, params: DocumentRangeFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let documents = self.documents.read().await;
        Ok(documents.get(&params.text_document.uri).map(|d| formatting::edits(d, Some(params.range))))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let at = params.text_document_position_params;
        let documents = self.documents.read().await;
        Ok(documents.get(&at.text_document.uri).and_then(|d| hover::at(d, at.position)))
    }

    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        let at = params.text_document_position_params;
        let documents = self.documents.read().await;
        let found = documents.get(&at.text_document.uri).and_then(|d| navigation::definition(d, at.position));
        Ok(found.map(|range| GotoDefinitionResponse::Scalar(Location::new(at.text_document.uri, range))))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let at = params.text_document_position;
        let documents = self.documents.read().await;
        let Some(document) = documents.get(&at.text_document.uri) else { return Ok(None) };
        let ranges = navigation::references(document, at.position, params.context.include_declaration);
        Ok(Some(ranges.into_iter().map(|r| Location::new(at.text_document.uri.clone(), r)).collect()))
    }

    async fn prepare_rename(&self, params: TextDocumentPositionParams) -> Result<Option<PrepareRenameResponse>> {
        let documents = self.documents.read().await;
        let found = documents.get(&params.text_document.uri).and_then(|d| navigation::prepare_rename(d, params.position));
        Ok(found.map(|(range, placeholder)| PrepareRenameResponse::RangeWithPlaceholder { range, placeholder }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let at = params.text_document_position;
        let documents = self.documents.read().await;
        let Some(document) = documents.get(&at.text_document.uri) else { return Ok(None) };
        let edits: Vec<TextEdit> =
            navigation::rename(document, at.position, &params.new_name).map_err(Error::invalid_params)?;
        Ok(Some(WorkspaceEdit { changes: Some(HashMap::from([(at.text_document.uri, edits)])), ..WorkspaceEdit::default() }))
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        self.publish(uri, None, Vec::new()).await;
    }
}
