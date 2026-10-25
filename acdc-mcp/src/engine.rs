use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use acdc_parser::{AttributeValue, Block, InlineMacro, InlineNode};
use ignore::WalkBuilder;
use nucleo_matcher::{
    Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use rmcp::schemars::{self, JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
    config::{Config, is_asciidoc},
    document::{Document, Entry, select},
    error::{Error, Result},
    paging::{cursor_timestamp, paginate},
    render::{self, Format},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
struct Common {
    /// Attributes overriding startup attributes for this request. Strings set values; booleans set or unset presence.
    #[serde(default, deserialize_with = "deserialize_attributes")]
    #[schemars(schema_with = "attributes_schema")]
    attributes: BTreeMap<String, AttributeValue<'static>>,
    /// Reject recovered or omitted source content. Ordinary presentation warnings remain usable.
    #[serde(default)]
    strict: bool,
    /// Response budget, estimated as serialized UTF-8 bytes / 4. Default 2000; range 256..16000.
    max_tokens: Option<usize>,
    /// Continuation from an identical request. Content changes invalidate the cursor.
    cursor: Option<String>,
}

fn deserialize_attributes<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, AttributeValue<'static>>, D::Error>
where
    D: Deserializer<'de>,
{
    BTreeMap::<String, Value>::deserialize(deserializer)?
        .into_iter()
        .map(|(name, value)| match value {
            Value::String(value) => Ok((name, value.into())),
            Value::Bool(value) => Ok((name, value.into())),
            _ => Err(de::Error::custom(
                "attribute values must be strings or booleans",
            )),
        })
        .collect()
}

fn attributes_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "object",
        "additionalProperties": {"anyOf": [{"type": "string"}, {"type": "boolean"}]},
    })
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub(super) struct FileRequest {
    /// Absolute document path, or a path relative to a configured root.
    path: String,
    #[serde(flatten)]
    common: Common,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub(super) struct ReadRequest {
    /// Absolute document path, or a path relative to a configured root.
    path: String,
    /// Selector from `adoc_tree`/`adoc_find`, exact ID, heading path (Setup > Installation), or fuzzy title. `@document` and `@preamble` read unsectioned content.
    selector: String,
    /// Processed, normalized `AsciiDoc` or readable plain text. Default asciidoc.
    #[serde(default)]
    format: Format,
    /// Include descendant sections. Default true. Ignored by `adoc_block`.
    #[serde(default = "default_true")]
    include_subsections: bool,
    #[serde(flatten)]
    common: Common,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub(super) struct FindRequest {
    /// Short fuzzy search terms or abbreviations; every space-separated term must match.
    /// Searches document structure and metadata, excluding body prose.
    query: String,
    /// Optional directory within the configured roots. Omit to search all roots.
    scope: Option<String>,
    #[serde(flatten)]
    common: Common,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub(super) struct ResolveRequest {
    /// Document containing the reference. Included targets can resolve within this assembled document.
    path: String,
    /// Target returned by `adoc_links`, such as #installation or guide.adoc#setup.
    target: String,
    /// Original source file for a relative link in an include. Use the link's source.file when present.
    source_path: Option<String>,
    #[serde(flatten)]
    common: Common,
}

pub(super) struct Engine {
    config: Arc<Config>,
    cancelled: CancellationToken,
    timestamp: u64,
}

impl Engine {
    pub(super) fn new(config: Arc<Config>, cancelled: CancellationToken) -> Self {
        Self {
            config,
            cancelled,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    fn timestamp(&self, common: &Common) -> Result<u64> {
        Ok(cursor_timestamp(common.cursor.as_deref())?.unwrap_or(self.timestamp))
    }

    fn load(&self, path: &str, common: &Common) -> Result<Document> {
        Document::load(
            &self.config,
            Path::new(path),
            &common.attributes,
            common.strict,
            &self.cancelled,
            self.timestamp(common)?,
        )
    }

    fn page(
        &self,
        tool: &str,
        request: &impl Serialize,
        common: &Common,
        items: &[Value],
        revision: &[u8],
    ) -> Result<String> {
        let mut identity = serde_json::to_value(request)?;
        if let Some(object) = identity.as_object_mut() {
            object.remove("cursor");
            object.remove("max_tokens");
        }
        let mut identity = serde_json::to_vec(
            &json!({"tool":tool,"request":identity,"roots":self.config.roots(),"attributes":self.config.attributes()}),
        )?;
        identity.extend_from_slice(revision);
        identity.extend_from_slice(&self.timestamp(common)?.to_be_bytes());
        let warning_count = items
            .iter()
            .filter(|item| item["kind"] == "warning")
            .count();
        paginate(
            items,
            &identity,
            warning_count,
            common.max_tokens,
            common.cursor.as_deref(),
            self.timestamp(common)?,
        )
    }

    pub(super) fn tree(&self, request: &FileRequest) -> Result<String> {
        let document = self.load(&request.path, &request.common)?;
        let mut items = document.warnings();
        items.push(json!({"kind":"document", "path":document.path(), "title":document.title(), "selector":"@document", "preamble_selector":"@preamble"}));
        for entry in document.entries()? {
            if entry.navigable() {
                items.push(document.describe(&entry));
            }
        }
        self.page(
            "tree",
            request,
            &request.common,
            &items,
            document.revision(),
        )
    }

    pub(super) fn metadata(&self, request: &FileRequest) -> Result<String> {
        let document = self.load(&request.path, &request.common)?;
        let mut items = document.warnings();
        let header = document.parsed().document().header.as_ref();
        items.push(json!({
            "kind":"document", "path":document.path(), "title":document.title(),
            "subtitle":header.and_then(|h| h.subtitle.as_ref()).map(|s| render::inline_text(s, Format::Plain)),
            "authors":header.map(|h| &h.authors),
        }));
        let mut attributes = document
            .parsed()
            .document()
            .attributes
            .iter()
            .collect::<Vec<_>>();
        attributes.sort_by_key(|(name, _)| *name);
        for (name, value) in attributes {
            let value = if value.is_presence() {
                json!(true)
            } else if let Some(integer) = value.as_integer() {
                json!(integer.to_string())
            } else {
                json!(value.as_str().unwrap_or_default())
            };
            items.push(json!({"kind":"attribute", "name":name, "value":value}));
        }
        self.page(
            "metadata",
            request,
            &request.common,
            &items,
            document.revision(),
        )
    }

    pub(super) fn read(&self, request: &ReadRequest, section: bool) -> Result<String> {
        if request.selector.trim().is_empty() {
            return Err(Error::Input("selector must not be empty".into()));
        }
        let document = self.load(&request.path, &request.common)?;
        let entries = document.entries()?;
        let mut items = document.warnings();
        if section && matches!(request.selector.as_str(), "@document" | "@preamble") {
            items.push(json!({"kind":"selection", "path":document.path(), "selector":request.selector, "title":document.title()}));
            if request.selector == "@document" && !document.title().is_empty() {
                let heading = if request.format == Format::Asciidoc {
                    format!("= {}", document.title())
                } else {
                    document.title()
                };
                items.push(json!({"kind":"content", "format":request.format, "text":format!("{heading}\n\n")}));
            }
            for block in &document.parsed().document().blocks {
                if request.selector == "@preamble" && matches!(block, Block::Section(_)) {
                    break;
                }
                if request.include_subsections || !matches!(block, Block::Section(_)) {
                    emit_content(
                        block,
                        request.format,
                        request.include_subsections,
                        &mut items,
                    );
                }
            }
            return self.page(
                "section",
                request,
                &request.common,
                &items,
                document.revision(),
            );
        }
        let matches = select(&entries, &request.selector, section);
        match matches.as_slice() {
            [entry] => {
                let mut description = document.describe(entry);
                description["kind"] = json!("selection");
                items.push(description);
                emit_content(entry.block(), request.format, request.include_subsections, &mut items);
            }
            [] => items.push(json!({"kind":"selection", "status":"not_found", "message":"No matching target. Use adoc_tree or adoc_find to select a target."})),
            _ => {
                items.push(json!({"kind":"selection", "status":"ambiguous", "message":"Multiple targets match. Repeat with an exact selector from the candidates."}));
                items.extend(matches.into_iter().map(|entry| document.describe(entry)));
            }
        }
        self.page(
            if section { "section" } else { "block" },
            request,
            &request.common,
            &items,
            document.revision(),
        )
    }

    fn discover(
        &self,
        scope: Option<&str>,
        warnings: &mut Vec<Value>,
    ) -> Result<BTreeSet<PathBuf>> {
        let scopes = if let Some(scope) = scope {
            vec![self.config.resolve(Path::new(scope))?]
        } else {
            self.config.roots().to_vec()
        };
        let mut files = BTreeSet::new();
        for scope in scopes {
            if !scope.is_dir() {
                return Err(Error::Input("search scope must be a directory".into()));
            }
            let walker = WalkBuilder::new(scope)
                .follow_links(false)
                .git_global(false)
                .require_git(false)
                .build();
            for entry in walker {
                if self.cancelled.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                match entry {
                    Ok(entry)
                        if entry.file_type().is_some_and(|kind| kind.is_file())
                            && is_asciidoc(entry.path()) =>
                    {
                        files.insert(self.config.check_path(entry.path())?);
                    }
                    Err(error) => {
                        warnings.push(json!({"kind":"warning", "message":error.to_string()}));
                    }
                    _ => {}
                }
            }
        }
        Ok(files)
    }

    pub(super) fn find(&self, request: &FindRequest) -> Result<String> {
        if request.query.trim().is_empty() {
            return Err(Error::Input("query must not be empty".into()));
        }
        let mut warnings = Vec::new();
        let files = self.discover(request.scope.as_deref(), &mut warnings)?;
        let pattern = Pattern::new(
            request.query.trim(),
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::default();
        let mut buffer = Vec::new();
        let mut results = Vec::new();
        let mut revision = Vec::new();
        for path in &files {
            if self.cancelled.is_cancelled() {
                return Err(Error::Cancelled);
            }
            revision.extend_from_slice(path.as_os_str().as_encoded_bytes());
            let document = match self.load(&path.to_string_lossy(), &request.common) {
                Ok(document) => document,
                Err(error) => {
                    warnings
                        .push(json!({"kind":"warning", "path":path,"message":error.to_string()}));
                    continue;
                }
            };
            revision.extend_from_slice(document.revision());
            warnings.extend(document.warnings());
            let attributes = &document.parsed().document().attributes;
            let tags = ["tags", "keywords", "description"]
                .iter()
                .filter_map(|name| attributes.get(name).and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join(" ");
            let context = format!(
                "{} {} {tags}",
                document.title(),
                path.file_stem().unwrap_or_default().to_string_lossy()
            );
            let document_score = pattern.score(Utf32Str::new(&context, &mut buffer), &mut matcher);
            let entries = document.entries()?;
            let mut found = false;
            for entry in entries.iter().filter(|entry| entry.navigable()) {
                let Some(score) =
                    pattern.score(Utf32Str::new(&entry.title(), &mut buffer), &mut matcher)
                else {
                    continue;
                };
                found = true;
                let mut result = document.describe(entry);
                result["kind"] = json!("match");
                result["target_kind"] = json!(render::kind(entry.block()));
                result["score"] = json!(score);
                results.push(result);
            }
            if !found && let Some(score) = document_score {
                results.push(json!({"kind":"match", "target_kind":"document", "path":document.path(), "title":document.title(), "selector":"@document", "score":score}));
            }
        }
        results.sort_by(|a, b| {
            b["score"]
                .as_u64()
                .cmp(&a["score"].as_u64())
                .then_with(|| a["path"].as_str().cmp(&b["path"].as_str()))
                .then_with(|| a["selector"].as_str().cmp(&b["selector"].as_str()))
        });
        let mut items = vec![
            json!({"kind":"search", "documents_scanned":files.len(), "matches":results.len()}),
        ];
        items.extend(warnings);
        items.extend(results);
        self.page("find", request, &request.common, &items, &revision)
    }

    pub(super) fn links(&self, request: &FileRequest) -> Result<String> {
        let document = self.load(&request.path, &request.common)?;
        let entries = document.entries()?;
        let mut items = document.warnings();
        if let Some(header) = &document.parsed().document().header {
            collect_links(&document, &header.title, &mut items);
            if let Some(subtitle) = &header.subtitle {
                collect_links(&document, subtitle, &mut items);
            }
        }
        for entry in &entries {
            if let Some(metadata) = entry.block().metadata()
                && let Some(link) = metadata.attributes.get_string("link")
            {
                items.push(json!({"kind":"link", "link_type":"link", "target":link,"path":document.path(),"source":document.source(entry.block().location())}));
            }
            for nodes in own_inlines(entry.block()) {
                collect_links(&document, nodes, &mut items);
            }
        }
        self.page(
            "links",
            request,
            &request.common,
            &items,
            document.revision(),
        )
    }

    pub(super) fn resolve(&self, request: &ResolveRequest) -> Result<String> {
        let document = self.load(&request.path, &request.common)?;
        let mut items = document.warnings();
        let mut revision = document.revision().to_vec();
        let target = request.target.trim();
        if target.is_empty() {
            return Err(Error::Input("target must not be empty".into()));
        }
        if let Some((scheme, _)) = target.split_once(':')
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
            && scheme.len() > 1
        {
            items.push(json!({"kind":"external", "target":target}));
        } else if target.starts_with('#')
            || document.parsed().document().references.contains_key(target)
        {
            resolve_anchor(&document, target.trim_start_matches('#'), &mut items)?;
        } else {
            let (filename, fragment) = target.split_once('#').unwrap_or((target, ""));
            let origin = request
                .source_path
                .as_deref()
                .map(Path::new)
                .map(|path| self.config.resolve(path))
                .transpose()?
                .unwrap_or_else(|| document.path().to_owned());
            let candidate = origin.parent().expect("file parent").join(filename);
            let mut candidates = Vec::new();
            if candidate
                .extension()
                .is_some_and(|extension| extension == "html")
            {
                candidates.extend(
                    ["adoc", "asciidoc", "asc"]
                        .map(|extension| candidate.with_extension(extension)),
                );
            }
            candidates.push(candidate);
            let existing = candidates.into_iter().find(|candidate| candidate.is_file());
            if let Some(existing) = existing {
                let path = self.config.check_path(&existing)?;
                if is_asciidoc(&path) {
                    let target_document = self.load(&path.to_string_lossy(), &request.common)?;
                    revision.extend_from_slice(target_document.revision());
                    items.extend(target_document.warnings());
                    if fragment.is_empty() {
                        items.push(json!({"kind":"resolved", "path":path, "title":target_document.title(), "selector":"@document", "target_kind":"document"}));
                    } else {
                        resolve_anchor(&target_document, fragment, &mut items)?;
                    }
                } else {
                    items.push(json!({"kind":"resolved_file", "path":path, "fragment":fragment}));
                }
            } else {
                items.push(json!({"kind":"unresolved", "target":target, "message":"No local target found"}));
            }
        }
        self.page("resolve", request, &request.common, &items, &revision)
    }
}

fn emit_content(
    block: &Block<'_>,
    format: Format,
    include_subsections: bool,
    items: &mut Vec<Value>,
) {
    if let Block::Section(section) = block {
        let title = render::inline_text(&section.title, format);
        let heading = if format == Format::Asciidoc {
            format!(
                "[#{}]\n{} {title}",
                section.id(),
                "=".repeat(usize::from(section.level) + 1)
            )
        } else {
            title
        };
        items.push(json!({"kind":"content","format":format,"text":format!("{heading}\n\n")}));
        for block in &section.content {
            if include_subsections || !matches!(block, Block::Section(_)) {
                emit_content(block, format, include_subsections, items);
            }
        }
    } else {
        let text = render::render_block(block, format, include_subsections);
        if !text.is_empty() {
            items.push(json!({"kind":"content","format":format,"text":format!("{text}\n\n")}));
        }
    }
}

fn resolve_anchor(document: &Document, id: &str, items: &mut Vec<Value>) -> Result<()> {
    if let Some(reference) = document.parsed().document().references.get(id) {
        let entries = document.entries()?;
        let matching = entries
            .iter()
            .filter(|entry| entry.has_id(id))
            .collect::<Vec<_>>();
        if matching.len() > 1 {
            items.push(json!({"kind":"ambiguous", "target":id}));
            items.extend(matching.iter().map(|entry| document.describe(entry)));
        } else {
            let location = &reference.location;
            let owner = matching.first().copied().or_else(|| {
                entries.iter().rev().find(|entry| {
                    let candidate = entry.block().location();
                    candidate.start.file == location.start.file
                        && candidate.end.file == location.end.file
                        && (candidate.start.line, candidate.start.column)
                            <= (location.start.line, location.start.column)
                        && (candidate.end.line, candidate.end.column)
                            >= (location.end.line, location.end.column)
                })
            });
            let selector = if id.is_empty() {
                Some("@document")
            } else {
                owner.map(Entry::selector)
            };
            items.push(json!({"kind":"resolved", "path":document.path(), "id":id,
                "selector":selector, "target_kind":owner.map(|entry| render::kind(entry.block())),
                "title":reference.title.as_ref().map(|title| render::inline_text(title, Format::Plain)),
                "source":document.source(&reference.location),
            }));
        }
    } else {
        items.push(json!({"kind":"unresolved", "target":id}));
    }
    Ok(())
}

fn own_inlines<'a>(block: &'a Block<'a>) -> Vec<&'a [InlineNode<'a>]> {
    let mut groups = Vec::new();
    if let Some(title) = render::title(block) {
        groups.push(title.as_ref());
    }
    if let Some(metadata) = block.metadata() {
        if let Some(attribution) = &metadata.attribution {
            groups.push(attribution.as_ref());
        }
        if let Some(citetitle) = &metadata.citetitle {
            groups.push(citetitle.as_ref());
        }
    }
    match block {
        Block::Paragraph(p) => groups.push(&p.content),
        Block::UnorderedList(l) => groups.extend(l.items.iter().map(|i| i.principal.as_slice())),
        Block::OrderedList(l) => groups.extend(l.items.iter().map(|i| i.principal.as_slice())),
        Block::CalloutList(l) => groups.extend(l.items.iter().map(|i| i.principal.as_slice())),
        Block::DescriptionList(l) => {
            for item in &l.items {
                groups.push(&item.term);
                groups.push(&item.principal_text);
            }
        }
        Block::DelimitedBlock(d) => match &d.inner {
            acdc_parser::DelimitedBlockType::DelimitedListing(nodes)
            | acdc_parser::DelimitedBlockType::DelimitedLiteral(nodes)
            | acdc_parser::DelimitedBlockType::DelimitedVerse(nodes)
            | acdc_parser::DelimitedBlockType::DelimitedPass(nodes) => groups.push(nodes),
            _ => {}
        },
        _ => {}
    }
    groups
}

fn collect_links(document: &Document, nodes: &[InlineNode<'_>], items: &mut Vec<Value>) {
    for node in nodes {
        let mut link = None;
        let children = match node {
            InlineNode::BoldText(n) => Some(n.content.as_slice()),
            InlineNode::ItalicText(n) => Some(n.content.as_slice()),
            InlineNode::MonospaceText(n) => Some(n.content.as_slice()),
            InlineNode::HighlightText(n) => Some(n.content.as_slice()),
            InlineNode::SubscriptText(n) => Some(n.content.as_slice()),
            InlineNode::SuperscriptText(n) => Some(n.content.as_slice()),
            InlineNode::CurvedQuotationText(n) => Some(n.content.as_slice()),
            InlineNode::CurvedApostropheText(n) => Some(n.content.as_slice()),
            InlineNode::Macro(m) => match m {
                InlineMacro::Link(l) => {
                    link = Some(("link", l.target.to_string()));
                    Some(l.text.as_slice())
                }
                InlineMacro::Url(l) => {
                    link = Some(("url", l.target.to_string()));
                    Some(l.text.as_slice())
                }
                InlineMacro::Mailto(l) => {
                    link = Some(("mailto", format!("mailto:{}", l.target)));
                    Some(l.text.as_slice())
                }
                InlineMacro::Autolink(l) => {
                    link = Some(("url", l.url.to_string()));
                    None
                }
                InlineMacro::CrossReference(l) => {
                    link = Some((
                        "xref",
                        if l.target_is_local {
                            format!("#{}", l.target)
                        } else {
                            l.target.to_owned()
                        },
                    ));
                    Some(l.text.as_slice())
                }
                InlineMacro::Footnote(f) => Some(f.content.as_slice()),
                _ => None,
            },
            _ => None,
        };
        if let Some((kind, target)) = link {
            items.push(json!({"kind":"link", "link_type":kind,"target":target,"path":document.path(),"source":document.source(node.location())}));
        }
        if let Some(children) = children {
            collect_links(document, children, items);
        }
    }
}

#[cfg(test)]
mod tests;
