//! LSP backend implementation
//!
//! Contains the main `Backend` struct that implements the `LanguageServer` trait.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbolParams,
    DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams, OneOf,
    ReferenceParams, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer};

use crate::capabilities::{completion, definition, hover, references, symbols};
use crate::state::Workspace;

/// LSP backend for `AsciiDoc` documents
pub struct Backend {
    /// Client handle for sending messages back to the editor
    client: Client,
    /// Workspace state management
    workspace: Workspace,
}

impl Backend {
    /// Create a new backend instance
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            workspace: Workspace::new(),
        }
    }

    /// Publish diagnostics for a document
    async fn publish_diagnostics(&self, uri: Url) {
        if let Some(doc) = self.workspace.get_document(&uri) {
            self.client
                .publish_diagnostics(uri, doc.diagnostics.clone(), Some(doc.version))
                .await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("Initializing acdc-lsp");

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Full sync for MVP simplicity - we get complete document on each change
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Enable document outline
                document_symbol_provider: Some(OneOf::Left(true)),
                // Enable go-to-definition for xrefs
                definition_provider: Some(OneOf::Left(true)),
                // Enable hover for xrefs, anchors, and links
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Enable find references for anchors and xrefs
                references_provider: Some(OneOf::Left(true)),
                // Enable completion for xrefs, attributes, and includes
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "<".to_string(), // for <<
                        ":".to_string(), // for xref: and attributes
                        "{".to_string(), // for attribute references
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "acdc-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("acdc-lsp initialized");
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down acdc-lsp");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let version = params.text_document.version;

        tracing::debug!("Document opened: {uri}");

        self.workspace.update_document(uri.clone(), text, version);
        self.publish_diagnostics(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // With FULL sync, we get the complete new text
        if let Some(change) = params.content_changes.into_iter().next() {
            tracing::debug!("Document changed: {uri}");

            self.workspace
                .update_document(uri.clone(), change.text, version);
            self.publish_diagnostics(uri).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        tracing::debug!("Document closed: {uri}");

        self.workspace.remove_document(&uri);
        // Clear diagnostics for closed file
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        // Get document and extract symbols while the guard is held
        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            doc.ast
                .as_ref()
                .map(|ast| DocumentSymbolResponse::Nested(symbols::document_symbols(ast)))
        } else {
            None
        };

        Ok(response)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Get document and find definition while the guard is held
        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            definition::find_definition_at_position(&doc, position).map(|loc| {
                GotoDefinitionResponse::Scalar(tower_lsp::lsp_types::Location {
                    uri: uri.clone(),
                    range: crate::convert::location_to_range(&loc),
                })
            })
        } else {
            None
        };

        Ok(response)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            hover::compute_hover(&doc, position)
        } else {
            None
        };

        Ok(response)
    }

    async fn references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<tower_lsp::lsp_types::Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            references::find_references(&doc, position, include_declaration).map(|ranges| {
                ranges
                    .into_iter()
                    .map(|range| tower_lsp::lsp_types::Location {
                        uri: uri.clone(),
                        range,
                    })
                    .collect()
            })
        } else {
            None
        };

        Ok(response)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            completion::compute_completions(&doc, position).map(CompletionResponse::Array)
        } else {
            None
        };

        Ok(response)
    }
}
