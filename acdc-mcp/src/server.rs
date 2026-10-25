use std::sync::Arc;

use rmcp::{
    RoleServer, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use tokio::sync::Semaphore;

use crate::{
    config::Config,
    engine::{Engine, FileRequest, FindRequest, ReadRequest, ResolveRequest},
    error::Result as ReaderResult,
};

#[derive(Clone)]
pub(super) struct Reader {
    config: Arc<Config>,
    workers: Arc<Semaphore>,
}

impl Reader {
    pub(super) fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            workers: Arc::new(Semaphore::new(2)),
        }
    }

    async fn run(
        &self,
        context: RequestContext<RoleServer>,
        operation: impl FnOnce(Engine) -> ReaderResult<String> + Send + 'static,
    ) -> CallToolResult {
        let Ok(permit) = self.workers.clone().try_acquire_owned() else {
            return error("Server is busy. Retry after an active request completes.");
        };
        let engine = Engine::new(self.config.clone(), context.ct);
        match tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation(engine)
        })
        .await
        {
            Ok(Ok(text)) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Ok(Err(failure)) => error(&failure.to_string()),
            Err(failure) => error(&format!("request worker failed: {failure}")),
        }
    }
}

fn error(message: &str) -> CallToolResult {
    let mut message = message.chars().take(512).collect::<String>();
    if message.len() > 900 {
        message.truncate(message.floor_char_boundary(900));
    }
    CallToolResult::error(vec![ContentBlock::text(message)])
}

#[tool_router]
impl Reader {
    #[tool(
        description = "Show an assembled AsciiDoc document's section hierarchy and addressable blocks, with selectors and original source locations. Includes local include content. Read with adoc_section or adoc_block.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn adoc_tree(
        &self,
        Parameters(request): Parameters<FileRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(context, move |engine| engine.tree(&request)).await
    }

    #[tool(
        description = "Find sections, titled blocks, or documents across configured repositories using case-insensitive fuzzy matching. Use short terms or abbreviations; every space-separated term must match. Searches headings, document titles, filenames, tags, keywords, and descriptions. No full-text search. Stateless: discovers and parses documents on each request. Use the returned path and selector with adoc_section or adoc_block.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn adoc_find(
        &self,
        Parameters(request): Parameters<FindRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(context, move |engine| engine.find(&request)).await
    }

    #[tool(
        description = "Read an AsciiDoc section by selector, ID, heading path, or fuzzy title. Includes subsections by default. Returns normalized processed AsciiDoc or plain text. Ambiguous matches return candidates. Concatenate content records in order. Repeat the identical request with next_cursor to continue.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn adoc_section(
        &self,
        Parameters(request): Parameters<ReadRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(context, move |engine| engine.read(&request, true))
            .await
    }

    #[tool(
        description = "Read an individual code listing, table, paragraph, or other non-section block by selector, ID, or title. Returns normalized processed AsciiDoc or plain text. Use adoc_tree to discover selectors.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn adoc_block(
        &self,
        Parameters(request): Parameters<ReadRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(context, move |engine| engine.read(&request, false))
            .await
    }

    #[tool(
        description = "Read document title, subtitle, authors, revision attributes, and effective end-of-header attributes. Later body attribute changes are scoped by the parser when reading content.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn adoc_metadata(
        &self,
        Parameters(request): Parameters<FileRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(context, move |engine| engine.metadata(&request))
            .await
    }

    #[tool(
        description = "List links and cross-references in an assembled document with their original source locations. Local xref targets start with #. Use adoc_resolve to navigate; this does not calculate backlinks or a graph.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn adoc_links(
        &self,
        Parameters(request): Parameters<FileRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(context, move |engine| engine.links(&request))
            .await
    }

    #[tool(
        description = "Resolve an anchor or local file reference to its document, selector, and source location. Resolves standard AsciiDoc paths; no Antora resource IDs. External URLs are reported without fetching them.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn adoc_resolve(
        &self,
        Parameters(request): Parameters<ResolveRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(context, move |engine| engine.resolve(&request))
            .await
    }
}

#[tool_handler(
    name = "acdc-mcp",
    instructions = "Find topics with adoc_find, inspect a file with adoc_tree, then read with adoc_section/adoc_block. Paths are absolute or relative to configured roots. A book includes local sources; the same files can be opened independently. Both asciidoc and plain output are available. Results are JSON pages in text content. max_tokens bounds serialized page bytes using 4 bytes per estimated token, including metadata and warnings. Continue with the identical arguments and next_cursor; max_tokens can change. Join content.text values in order. Oversized content continues in readable content records. Other oversized records use kind=fragment: concatenate text by record_index and offset, then parse that JSON record. Cursors require unchanged inputs and processing options. No persistent index or document cache, full-text search, graph queries, Antora, or network fetching."
)]
impl ServerHandler for Reader {
    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<rmcp::model::ListToolsResult, rmcp::ErrorData>> + Send {
        std::future::ready(Ok(rmcp::model::ListToolsResult::with_all_items(
            Self::tool_router().list_all(),
        )))
    }
}
