//! LSP backend implementation
//!
//! Contains the main `Backend` struct that implements the `LanguageServer` trait.

use std::sync::{Mutex, PoisonError, RwLock};

use tower_lsp_server::jsonrpc::{Error, Result};
use tower_lsp_server::ls_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CallHierarchyServerCapability, CodeActionOptions, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CodeLens, CodeLensOptions, CodeLensParams,
    CompletionOptions, CompletionParams, CompletionResponse, ConfigurationItem,
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWorkspaceFoldersParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams, DocumentLink,
    DocumentLinkOptions, DocumentLinkParams, DocumentOnTypeFormattingOptions,
    DocumentOnTypeFormattingParams, DocumentRangeFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, FileOperationFilter, FileOperationPattern, FileOperationPatternKind,
    FileOperationRegistrationOptions, FoldingRange, FoldingRangeParams,
    FoldingRangeProviderCapability, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
    InlayHint, InlayHintParams, MessageType, OneOf, PrepareRenameResponse, ReferenceParams,
    Registration, RenameFilesParams, RenameOptions, RenameParams, SelectionRange,
    SelectionRangeParams, SelectionRangeProviderCapability, SemanticTokensParams,
    SemanticTokensResult, ServerCapabilities, ServerInfo, SignatureHelp, SignatureHelpOptions,
    SignatureHelpParams, SymbolInformation, TextDocumentPositionParams, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Uri, WorkDoneProgressOptions, WorkspaceEdit,
    WorkspaceFileOperationsServerCapabilities, WorkspaceFoldersServerCapabilities,
    WorkspaceServerCapabilities, WorkspaceSymbolParams, WorkspaceSymbolResponse,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::capabilities::{
    call_hierarchy, code_actions, code_lens, completion, definition, document_links, file_rename,
    folding, formatting, hover, inlay_hints, on_type_formatting, references, rename,
    selection_range, semantic_tokens, signature_help, symbols,
};
use crate::config::{
    AnalysisConfiguration, BackendUpdate, RootConfiguration, ServerOptions, WorkspaceSettings,
    parse_backend_update,
};
use crate::state::Workspace;

#[derive(Clone, Copy, Default)]
struct ClientFeatures {
    configuration: ConfigurationSupport,
    refresh: RefreshSupport,
}

#[derive(Clone, Copy, Default)]
struct ConfigurationSupport {
    pull: bool,
    dynamic_registration: bool,
}

#[derive(Clone, Copy, Default)]
struct RefreshSupport {
    semantic_tokens: bool,
    code_lens: bool,
    inlay_hints: bool,
}

/// LSP backend for `AsciiDoc` documents
pub struct Backend {
    /// Client handle for sending messages back to the editor
    client: Client,
    /// Workspace state management
    workspace: Workspace,
    /// Serializes mutations that rebuild parsed workspace state.
    mutation: Mutex<()>,
    /// Client capabilities captured during initialization.
    client_features: RwLock<ClientFeatures>,
}

impl Backend {
    /// Create a new backend instance
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            workspace: Workspace::new(),
            mutation: Mutex::new(()),
            client_features: RwLock::new(ClientFeatures::default()),
        }
    }

    /// Publish diagnostics for a document
    async fn publish_diagnostics(&self, uri: Uri) {
        if let Some(doc) = self.workspace.get_document(&uri) {
            self.client
                .publish_diagnostics(uri, doc.diagnostics.clone(), Some(doc.version))
                .await;
        }
    }

    fn features(&self) -> ClientFeatures {
        *self
            .client_features
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn capture_client_features(&self, params: &InitializeParams) {
        let workspace = params.capabilities.workspace.as_ref();
        let features = ClientFeatures {
            configuration: ConfigurationSupport {
                pull: workspace
                    .and_then(|capabilities| capabilities.configuration)
                    .unwrap_or(false),
                dynamic_registration: workspace
                    .and_then(|capabilities| capabilities.did_change_configuration.as_ref())
                    .and_then(|capability| capability.dynamic_registration)
                    .unwrap_or(false),
            },
            refresh: RefreshSupport {
                semantic_tokens: workspace
                    .and_then(|capabilities| capabilities.semantic_tokens.as_ref())
                    .and_then(|capability| capability.refresh_support)
                    .unwrap_or(false),
                code_lens: workspace
                    .and_then(|capabilities| capabilities.code_lens.as_ref())
                    .and_then(|capability| capability.refresh_support)
                    .unwrap_or(false),
                inlay_hints: workspace
                    .and_then(|capabilities| capabilities.inlay_hint.as_ref())
                    .and_then(|capability| capability.refresh_support)
                    .unwrap_or(false),
            },
        };
        *self
            .client_features
            .write()
            .unwrap_or_else(PoisonError::into_inner) = features;
    }

    async fn pull_configuration(
        &self,
        mut configuration: AnalysisConfiguration,
    ) -> Option<AnalysisConfiguration> {
        let roots = configuration.roots();
        let mut items = Vec::with_capacity(roots.len() + 1);
        items.push(ConfigurationItem {
            scope_uri: None,
            section: Some("acdc-lsp".to_string()),
        });
        items.extend(roots.iter().cloned().map(|root| ConfigurationItem {
            scope_uri: Some(root),
            section: Some("acdc-lsp".to_string()),
        }));

        let values = match self.client.configuration(items).await {
            Ok(values) => values,
            Err(error) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("failed to load acdc-lsp workspace configuration: {error}"),
                    )
                    .await;
                return None;
            }
        };

        if let Some(value) = values.first() {
            match parse_workspace_settings(value) {
                Ok(settings) => configuration.set_unscoped(settings.backend),
                Err(error) => self.client.log_message(MessageType::WARNING, error).await,
            }
        } else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    "workspace/configuration omitted the unscoped acdc-lsp settings",
                )
                .await;
        }

        let mut root_configurations = Vec::with_capacity(roots.len());
        for (index, uri) in roots.into_iter().enumerate() {
            let previous = configuration.root_backend(&uri);
            let backend = if let Some(value) = values.get(index + 1) {
                match parse_workspace_settings(value) {
                    Ok(settings) => settings.backend,
                    Err(error) => {
                        self.client
                            .log_message(
                                MessageType::WARNING,
                                format!("{error} for workspace root {}", uri.as_str()),
                            )
                            .await;
                        previous
                    }
                }
            } else {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!(
                            "workspace/configuration omitted settings for workspace root {}",
                            uri.as_str()
                        ),
                    )
                    .await;
                previous
            };
            root_configurations.push(RootConfiguration { uri, backend });
        }
        configuration.replace_roots(root_configurations);
        Some(configuration)
    }

    async fn apply_configuration(&self, configuration: AnalysisConfiguration) {
        let result = {
            let _mutation = self.mutation.lock().unwrap_or_else(PoisonError::into_inner);
            self.workspace.apply_analysis_configuration(&configuration)
        };
        if !result.changed {
            return;
        }

        for uri in result.reparsed_documents {
            self.publish_diagnostics(uri).await;
        }

        let features = self.features();
        if features.refresh.semantic_tokens {
            let _ = self.client.semantic_tokens_refresh().await;
        }
        if features.refresh.code_lens {
            let _ = self.client.code_lens_refresh().await;
        }
        if features.refresh.inlay_hints {
            let _ = self.client.inlay_hint_refresh().await;
        }
    }
}

fn parse_workspace_settings(
    value: &serde_json::Value,
) -> std::result::Result<WorkspaceSettings, String> {
    if value.is_null() {
        return Ok(WorkspaceSettings::default());
    }
    serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid acdc-lsp workspace configuration: {error}"))
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("Initializing acdc-lsp");
        self.capture_client_features(&params);

        let options = params
            .initialization_options
            .map(serde_json::from_value::<ServerOptions>)
            .transpose()
            .map_err(|error| Error::invalid_params(error.to_string()))?
            .unwrap_or_default();
        tracing::info!(backend = ?options.backend, "configured analysis backend");

        // Capture workspace roots for cross-file resolution
        let mut roots = Vec::new();
        if let Some(folders) = params.workspace_folders {
            for folder in folders {
                roots.push(folder.uri);
            }
        } else if let Some(root_uri) = params.root_uri {
            roots.push(root_uri);
        }
        self.workspace.initialize_analysis(options.backend, roots);

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
                    tower_lsp_server::ls_types::SemanticTokensServerCapabilities::SemanticTokensOptions(
                        semantic_tokens::create_options(),
                    ),
                ),
                // Enable workspace symbol search
                workspace_symbol_provider: Some(OneOf::Left(true)),
                // Enable code actions (quick-fixes, refactorings, source actions)
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            tower_lsp_server::ls_types::CodeActionKind::QUICKFIX,
                            tower_lsp_server::ls_types::CodeActionKind::REFACTOR_EXTRACT,
                            tower_lsp_server::ls_types::CodeActionKind::SOURCE,
                        ]),
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                        resolve_provider: Some(false),
                    },
                )),
                // Enable document formatting
                document_formatting_provider: Some(OneOf::Left(true)),
                // Enable range formatting
                document_range_formatting_provider: Some(OneOf::Left(true)),
                // Enable on-type formatting for list continuation and block auto-close
                document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                    first_trigger_character: "\n".to_string(),
                    more_trigger_character: None,
                }),
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
                // Enable signature help for macro attribute lists
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["[".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
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
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
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
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "acdc-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("acdc-lsp initialized");
        let features = self.features();
        if features.configuration.dynamic_registration
            && let Err(error) = self
                .client
                .register_capability(vec![Registration {
                    id: "acdc-lsp-did-change-configuration".to_string(),
                    method: "workspace/didChangeConfiguration".to_string(),
                    register_options: None,
                }])
                .await
        {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("failed to register dynamic configuration support: {error}"),
                )
                .await;
        }
        if features.configuration.pull {
            let current = self.workspace.analysis_configuration();
            if let Some(configuration) = self.pull_configuration(current).await {
                self.apply_configuration(configuration).await;
            }
        }
        {
            let _mutation = self.mutation.lock().unwrap_or_else(PoisonError::into_inner);
            self.workspace.scan_workspace_files();
        }
        tracing::info!(
            indexed_files = self.workspace.symbol_index_len(),
            "workspace file scan complete"
        );
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down acdc-lsp");
        Ok(())
    }

    #[tracing::instrument(name = "lsp/didOpen", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let version = params.text_document.version;
        {
            let _mutation = self.mutation.lock().unwrap_or_else(PoisonError::into_inner);
            self.workspace.update_document(uri.clone(), text, version);
        }
        self.publish_diagnostics(uri).await;
    }

    #[tracing::instrument(name = "lsp/didChange", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // With FULL sync, we get the complete new text
        if let Some(change) = params.content_changes.into_iter().next() {
            {
                let _mutation = self.mutation.lock().unwrap_or_else(PoisonError::into_inner);
                self.workspace
                    .update_document(uri.clone(), change.text, version);
            }
            self.publish_diagnostics(uri).await;
        }
    }

    #[tracing::instrument(name = "lsp/didClose", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        {
            let _mutation = self.mutation.lock().unwrap_or_else(PoisonError::into_inner);
            self.workspace.remove_document(&uri);
        }
        // Clear diagnostics for closed file
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    #[tracing::instrument(name = "lsp/didChangeConfiguration", level = "debug", skip_all)]
    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        let features = self.features();
        if features.configuration.pull {
            let current = self.workspace.analysis_configuration();
            if let Some(configuration) = self.pull_configuration(current).await {
                self.apply_configuration(configuration).await;
            }
            return;
        }

        let update = match parse_backend_update(&params.settings) {
            Ok(update) => update,
            Err(error) => {
                self.client.log_message(MessageType::WARNING, error).await;
                return;
            }
        };
        let mut configuration = self.workspace.analysis_configuration();
        match update {
            BackendUpdate::Unchanged => return,
            BackendUpdate::Set(backend) => configuration.set_unscoped(Some(backend)),
            BackendUpdate::Reset => configuration.set_unscoped(None),
        }
        self.apply_configuration(configuration).await;
    }

    #[tracing::instrument(name = "lsp/didChangeWorkspaceFolders", level = "debug", skip_all, fields(added = params.event.added.len(), removed = params.event.removed.len()))]
    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let mut configuration = self.workspace.analysis_configuration();
        let removed: Vec<Uri> = params
            .event
            .removed
            .into_iter()
            .map(|folder| folder.uri)
            .collect();
        let mut roots: Vec<RootConfiguration> = configuration
            .roots()
            .into_iter()
            .filter(|root| !removed.contains(root))
            .map(|uri| RootConfiguration {
                backend: configuration.root_backend(&uri),
                uri,
            })
            .collect();
        for folder in params.event.added {
            if !roots.iter().any(|root| root.uri == folder.uri) {
                roots.push(RootConfiguration {
                    uri: folder.uri,
                    backend: None,
                });
            }
        }
        configuration.replace_roots(roots);

        if self.features().configuration.pull {
            let pulled = self
                .pull_configuration(configuration.clone())
                .await
                .unwrap_or(configuration);
            self.apply_configuration(pulled).await;
        } else {
            self.apply_configuration(configuration).await;
        }
    }

    #[tracing::instrument(name = "lsp/documentSymbol", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        // Get document and extract symbols while the guard is held
        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            doc.ast().map(|ast| {
                DocumentSymbolResponse::Nested(symbols::document_symbols(ast.document()))
            })
        } else {
            None
        };

        Ok(response)
    }

    #[tracing::instrument(name = "lsp/workspaceSymbol", level = "debug", skip_all, fields(query = %params.query))]
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<WorkspaceSymbolResponse>> {
        let query = &params.query;
        let results = self.workspace.query_workspace_symbols(query);

        let symbols: Vec<SymbolInformation> = results
            .into_iter()
            .map(|(uri, symbol)| SymbolInformation {
                name: symbol.name,
                kind: symbol.kind,
                location: tower_lsp_server::ls_types::Location {
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
            Ok(Some(WorkspaceSymbolResponse::Flat(symbols)))
        }
    }

    #[tracing::instrument(name = "lsp/gotoDefinition", level = "info", skip_all, fields(uri = params.text_document_position_params.text_document.uri.as_str()))]
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            let result =
                definition::find_definition_at_position(&doc, &uri, &self.workspace, position);
            tracing::info!(found = result.is_some(), "goto_definition result");
            result.map(|(target_uri, loc)| {
                tracing::info!(
                    target_uri = target_uri.as_str(),
                    ?loc,
                    "goto_definition resolved to"
                );
                GotoDefinitionResponse::Scalar(tower_lsp_server::ls_types::Location {
                    uri: target_uri,
                    range: crate::convert::location_to_range(&loc),
                })
            })
        } else {
            tracing::warn!(uri = uri.as_str(), "document not found in workspace");
            None
        };

        Ok(response)
    }

    #[tracing::instrument(name = "lsp/hover", level = "info", skip_all, fields(uri = params.text_document_position_params.text_document.uri.as_str()))]
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

    #[tracing::instrument(name = "lsp/references", level = "debug", skip_all, fields(uri = params.text_document_position.text_document.uri.as_str()))]
    async fn references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<tower_lsp_server::ls_types::Location>>> {
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

    #[tracing::instrument(name = "lsp/completion", level = "info", skip_all, fields(uri = params.text_document_position.text_document.uri.as_str()))]
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

    #[tracing::instrument(name = "lsp/signatureHelp", level = "debug", skip_all, fields(uri = params.text_document_position_params.text_document.uri.as_str()))]
    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            signature_help::compute_signature_help(&doc, position)
        } else {
            None
        };

        Ok(response)
    }

    #[tracing::instrument(name = "lsp/documentLink", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| document_links::collect_document_links(&doc, &uri));

        Ok(response)
    }

    #[tracing::instrument(name = "lsp/foldingRange", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            doc.ast()
                .map(|ast| folding::compute_folding_ranges(ast.document()))
        } else {
            None
        };

        Ok(response)
    }

    #[tracing::instrument(name = "lsp/prepareRename", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
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

    #[tracing::instrument(name = "lsp/rename", level = "debug", skip_all, fields(uri = params.text_document_position.text_document.uri.as_str()))]
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

    #[tracing::instrument(name = "lsp/codeAction", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;

        let response = self.workspace.get_document(&uri).map(|doc| {
            code_actions::compute_code_actions(&doc, &uri, params.range, &params.context)
        });

        Ok(response.filter(|actions| !actions.is_empty()))
    }

    #[tracing::instrument(name = "lsp/codeLens", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| code_lens::compute_code_lenses(&doc, &uri, &self.workspace));

        Ok(response.filter(|lenses| !lenses.is_empty()))
    }

    #[tracing::instrument(name = "lsp/semanticTokensFull", level = "info", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        let response = if let Some(doc) = self.workspace.get_document(&uri) {
            doc.ast().map(|ast| {
                SemanticTokensResult::Tokens(semantic_tokens::compute_semantic_tokens(
                    ast.document(),
                    &doc.conditionals,
                    doc.text(),
                ))
            })
        } else {
            None
        };

        Ok(response)
    }

    #[tracing::instrument(name = "lsp/formatting", level = "info", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| formatting::format_document(&doc, &params.options));

        Ok(response.filter(|edits| !edits.is_empty()))
    }

    #[tracing::instrument(name = "lsp/rangeFormatting", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
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

    #[tracing::instrument(name = "lsp/onTypeFormatting", level = "debug", skip_all, fields(uri = params.text_document_position.text_document.uri.as_str()))]
    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let response = self
            .workspace
            .get_document(&uri)
            .and_then(|doc| on_type_formatting::format_on_type(&doc, position, &params.ch));

        Ok(response)
    }

    #[tracing::instrument(name = "lsp/inlayHint", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;

        let response = self
            .workspace
            .get_document(&uri)
            .map(|doc| inlay_hints::compute_inlay_hints(&doc, &params.range));

        Ok(response.filter(|hints| !hints.is_empty()))
    }

    #[tracing::instrument(name = "lsp/selectionRange", level = "debug", skip_all, fields(uri = params.text_document.uri.as_str()))]
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

    #[tracing::instrument(name = "lsp/willRenameFiles", level = "debug", skip_all, fields(count = params.files.len()))]
    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        Ok(file_rename::compute_file_rename_edits(
            &self.workspace,
            &params.files,
        ))
    }

    #[tracing::instrument(name = "lsp/didRenameFiles", level = "debug", skip_all, fields(count = params.files.len()))]
    async fn did_rename_files(&self, params: RenameFilesParams) {
        let _mutation = self.mutation.lock().unwrap_or_else(PoisonError::into_inner);
        file_rename::update_workspace_after_rename(&self.workspace, &params.files);
    }

    #[tracing::instrument(name = "lsp/prepareCallHierarchy", level = "debug", skip_all, fields(uri = params.text_document_position_params.text_document.uri.as_str()))]
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

    #[tracing::instrument(name = "lsp/incomingCalls", level = "debug", skip_all)]
    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        Ok(call_hierarchy::incoming_calls(
            &params.item,
            &self.workspace,
        ))
    }

    #[tracing::instrument(name = "lsp/outgoingCalls", level = "debug", skip_all)]
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
