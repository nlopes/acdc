//! LSP backend implementation
//!
//! Contains the main `Backend` struct that implements the `LanguageServer` trait.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CallHierarchyServerCapability, CodeActionOptions, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CodeLens, CodeLensOptions, CodeLensParams,
    CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams, DocumentLink,
    DocumentLinkOptions, DocumentLinkParams, DocumentRangeFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, FileOperationFilter, FileOperationPattern, FileOperationPatternKind,
    FileOperationRegistrationOptions, FoldingRange, FoldingRangeParams,
    FoldingRangeProviderCapability, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
    InlayHint, InlayHintParams, OneOf, PrepareRenameResponse, ReferenceParams, RenameFilesParams,
    RenameOptions, RenameParams, SelectionRange, SelectionRangeParams,
    SelectionRangeProviderCapability, SemanticTokensParams, SemanticTokensResult,
    ServerCapabilities, ServerInfo, SymbolInformation, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url, WorkDoneProgressOptions,
    WorkspaceEdit, WorkspaceFileOperationsServerCapabilities, WorkspaceServerCapabilities,
    WorkspaceSymbolParams,
};
use tower_lsp::{Client, LanguageServer};

use crate::capabilities::{
    call_hierarchy, code_actions, code_lens, completion, definition, document_links, file_rename,
    folding, formatting, hover, inlay_hints, references, rename, selection_range, semantic_tokens,
    symbols,
};
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
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("Initializing acdc-lsp");

        // Capture workspace roots for cross-file resolution
        let mut roots = Vec::new();
        if let Some(folders) = params.workspace_folders {
            for folder in folders {
                roots.push(folder.uri);
            }
        } else if let Some(root_uri) = params.root_uri {
            roots.push(root_uri);
        }
        if !roots.is_empty() {
            self.workspace.set_workspace_roots(roots);
        }

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
                // Enable clickable links for URLs, includes, and images
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                // Enable folding for sections and delimited blocks
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                // Enable rename for anchor IDs and xrefs
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                // Enable semantic tokens for syntax highlighting
                semantic_tokens_provider: Some(
                    tower_lsp::lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(
                        semantic_tokens::create_options(),
                    ),
                ),
                // Enable workspace symbol search
                workspace_symbol_provider: Some(OneOf::Left(true)),
                // Enable code actions (quick-fixes, refactorings, source actions)
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            tower_lsp::lsp_types::CodeActionKind::QUICKFIX,
                            tower_lsp::lsp_types::CodeActionKind::REFACTOR_EXTRACT,
                            tower_lsp::lsp_types::CodeActionKind::SOURCE,
                        ]),
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                        resolve_provider: Some(false),
                    },
                )),
                // Enable document formatting
                document_formatting_provider: Some(OneOf::Left(true)),
                // Enable range formatting
                document_range_formatting_provider: Some(OneOf::Left(true)),
                // Enable code lens for reference counts
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                // Enable smart selection expansion
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                // Enable call hierarchy for include-tree navigation
                call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
                // Enable inlay hints for resolved attributes and xref titles
                inlay_hint_provider: Some(OneOf::Left(true)),
                // Enable completion for xrefs, attributes, and includes
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "<".to_string(), // for <<
                        ":".to_string(), // for xref: and attributes
                        "{".to_string(), // for attribute references
                    ]),
                    ..Default::default()
                }),
                // Enable automatic link updates on file rename
                workspace: Some(WorkspaceServerCapabilities {
                    file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                        will_rename: Some(FileOperationRegistrationOptions {
                            filters: vec![FileOperationFilter {
                                scheme: Some("file".to_string()),
                                pattern: FileOperationPattern {
                                    glob: "**/*.{adoc,asciidoc,asc}".to_string(),
                                    matches: Some(FileOperationPatternKind::File),
                                    options: None,
                                },
                            }],
                        }),
                        ..Default::default()
                    }),
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
        self.workspace.scan_workspace_files();
        tracing::info!(
            indexed_files = self.workspace.symbol_index_len(),
            "workspace file scan complete"
        );
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

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = &params.query;
        let results = self.workspace.query_workspace_symbols(query);

        let symbols: Vec<SymbolInformation> = results
            .into_iter()
            .map(|(uri, symbol)| SymbolInformation {
                name: symbol.name,
                kind: symbol.kind,
                location: tower_lsp::lsp_types::Location {
                    uri,
                    range: crate::convert::location_to_range(&symbol.location),
                },
                tags: None,
                deprecated: None,
                container_name: symbol.detail,
            })
            .collect();

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(symbols))
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        tracing::info!(%uri, ?position, "goto_definition request");
        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            let result =
                definition::find_definition_at_position(&doc, &uri, &self.workspace, position);
            tracing::info!(found = result.is_some(), "goto_definition result");
            result.map(|(target_uri, loc)| {
                tracing::info!(%target_uri, ?loc, "goto_definition resolved to");
                GotoDefinitionResponse::Scalar(tower_lsp::lsp_types::Location {
                    uri: target_uri,
                    range: crate::convert::location_to_range(&loc),
                })
            })
        } else {
            tracing::warn!(%uri, "document not found in workspace");
            None
        };

        Ok(response)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            hover::compute_hover(&doc, &uri, &self.workspace, position)
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
            references::find_references(&doc, &uri, &self.workspace, position, include_declaration)
        } else {
            None
        };

        Ok(response)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            completion::compute_completions(&doc, &uri, &self.workspace, position)
                .map(CompletionResponse::Array)
        } else {
            None
        };

        Ok(response)
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| document_links::collect_document_links(&doc, &uri));

        Ok(response)
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            doc.ast.as_ref().map(folding::compute_folding_ranges)
        } else {
            None
        };

        Ok(response)
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            rename::prepare_rename(&doc, position)
        } else {
            None
        };

        Ok(response)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            rename::compute_rename(&doc, &uri, &self.workspace, position, &new_name)
        } else {
            None
        };

        Ok(response)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;

        let response = self.workspace.get_document(&uri).map(|doc| {
            code_actions::compute_code_actions(&doc, &uri, params.range, &params.context)
        });

        Ok(response.filter(|actions| !actions.is_empty()))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| code_lens::compute_code_lenses(&doc, &uri, &self.workspace));

        Ok(response.filter(|lenses| !lenses.is_empty()))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            doc.ast.as_ref().map(|ast| {
                SemanticTokensResult::Tokens(semantic_tokens::compute_semantic_tokens(
                    ast,
                    &doc.conditionals,
                    &doc.text,
                ))
            })
        } else {
            None
        };

        Ok(response)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| formatting::format_document(&doc, &params.options));

        Ok(response.filter(|edits| !edits.is_empty()))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| formatting::format_range(&doc, &params.range, &params.options));

        Ok(response.filter(|edits| !edits.is_empty()))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| inlay_hints::compute_inlay_hints(&doc, &params.range));

        Ok(response.filter(|hints| !hints.is_empty()))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| selection_range::compute_selection_ranges(&doc, &params.positions));

        Ok(response)
    }

    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        tracing::info!(count = params.files.len(), "workspace/willRenameFiles");
        Ok(file_rename::compute_file_rename_edits(
            &self.workspace,
            &params.files,
        ))
    }

    async fn did_rename_files(&self, params: RenameFilesParams) {
        tracing::info!(count = params.files.len(), "workspace/didRenameFiles");
        file_rename::update_workspace_after_rename(&self.workspace, &params.files);
    }

    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            call_hierarchy::prepare_call_hierarchy(&doc, &uri, position)
        } else {
            None
        };

        Ok(response)
    }

    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        Ok(call_hierarchy::incoming_calls(
            &params.item,
            &self.workspace,
        ))
    }

    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        Ok(call_hierarchy::outgoing_calls(
            &params.item,
            &self.workspace,
        ))
    }
}
