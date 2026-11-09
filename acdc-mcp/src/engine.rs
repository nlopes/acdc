use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use acdc_parser::{
    AttributeValue,
    ast::{
        AttributeValueRef, AuthorRef, BlockKind, BlockRef, ContentRef, HeaderRef, InlineKind,
        InlineRef, PositionRef, StructuralRef,
    },
};
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
            &json!({"tool":tool,"request":identity,"roots":self.config.roots(),"attributes":self.config.attributes(),"compatibilityMode":self.config.compatibility_mode()}),
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
        let header = document.parsed().document().header();
        items.push(json!({
            "kind":"document", "path":document.path(), "title":document.title(),
            "subtitle":header.and_then(HeaderRef::subtitle).map(|s| render::inline_text(s, Format::Plain)),
            "authors":header.map(|h| h.authors().map(author).collect::<Vec<_>>()),
        }));
        let mut attributes = document
            .parsed()
            .document()
            .attributes()
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
            for block in document.parsed().document().blocks() {
                if request.selector == "@preamble" && matches!(block.kind(), BlockKind::Section(_))
                {
                    break;
                }
                if request.include_subsections || !matches!(block.kind(), BlockKind::Section(_)) {
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
            let attributes = document.parsed().document().attributes();
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
        // Apply the same depth and block limits as the other navigation tools.
        document.entries()?;
        let mut items = document.warnings();
        for node in document.parsed().document().descendants() {
            if let StructuralRef::Block(block) = node
                && let Some(attributes) = block.attributes()
                && let Some(AttributeValueRef::Text(link)) = attributes.get("link")
            {
                items.push(json!({"kind":"link", "link_type":"link", "target":link,"path":document.path(),"source":document.source(block.location())}));
            }
            for field in node.content_fields() {
                collect_links(&document, field.content(), &mut items);
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
            || document.parsed().document().references().contains(target)
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
    block: BlockRef<'_>,
    format: Format,
    include_subsections: bool,
    items: &mut Vec<Value>,
) {
    if let BlockKind::Section(section) = block.kind() {
        let title = section
            .title()
            .map(|title| render::inline_text(title, format))
            .unwrap_or_default();
        let heading = if format == Format::Asciidoc {
            let anchor = section
                .id()
                .map_or_else(String::new, |id| format!("[#{}]\n", id.as_str()));
            format!(
                "{anchor}{} {title}",
                "=".repeat(usize::from(section.level()) + 1)
            )
        } else {
            title
        };
        items.push(json!({"kind":"content","format":format,"text":format!("{heading}\n\n")}));
        for block in section.blocks() {
            if include_subsections || !matches!(block.kind(), BlockKind::Section(_)) {
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
    if let Some(reference) = document.parsed().document().references().get(id) {
        let entries = document.entries()?;
        let matching = entries
            .iter()
            .filter(|entry| entry.has_id(id))
            .collect::<Vec<_>>();
        if matching.len() > 1 {
            items.push(json!({"kind":"ambiguous", "target":id}));
            items.extend(matching.iter().map(|entry| document.describe(entry)));
        } else {
            let location = reference.location();
            let owner = matching.first().copied().or_else(|| {
                entries.iter().rev().find(|entry| {
                    let candidate = entry.block().location();
                    same_origin(candidate.start(), location.start())
                        && same_origin(candidate.end(), location.end())
                        && (candidate.start().line(), candidate.start().column())
                            <= (location.start().line(), location.start().column())
                        && (candidate.end().line(), candidate.end().column())
                            >= (location.end().line(), location.end().column())
                })
            });
            let selector = if id.is_empty() {
                Some("@document")
            } else {
                owner.map(Entry::selector)
            };
            items.push(json!({"kind":"resolved", "path":document.path(), "id":id,
                "selector":selector, "target_kind":owner.map(|entry| render::kind(entry.block())),
                "title":reference.title().map(|title| render::inline_text(title, Format::Plain)),
                "source":document.source(reference.location()),
            }));
        }
    } else {
        items.push(json!({"kind":"unresolved", "target":id}));
    }
    Ok(())
}

fn collect_links(document: &Document, nodes: ContentRef<'_>, items: &mut Vec<Value>) {
    for node in nodes.inlines() {
        collect_link(document, node, items);
        for field in node.content_occurrences() {
            for child in field.content().inlines() {
                collect_link(document, child, items);
            }
        }
    }
}

fn collect_link(document: &Document, node: InlineRef<'_>, items: &mut Vec<Value>) {
    let link = match node.kind() {
        InlineKind::Link(link) => Some(("link", link.target().to_string())),
        InlineKind::UrlMacro(link) => Some(("url", link.target().to_string())),
        InlineKind::Mailto(link) => Some(("mailto", link.target().to_string())),
        InlineKind::Autolink(link) => Some(("url", link.target().to_string())),
        InlineKind::CrossReference(link) => Some((
            "xref",
            if link.target_is_local() {
                format!("#{}", link.target())
            } else {
                link.target().to_owned()
            },
        )),
        InlineKind::Image(image) => image
            .attributes()
            .get("link")
            .and_then(|value| match value {
                AttributeValueRef::Text(target) => Some(("link", target.to_owned())),
                _ => None,
            }),
        InlineKind::Icon(icon) => icon.attributes().get("link").and_then(|value| match value {
            AttributeValueRef::Text(target) => Some(("link", target.to_owned())),
            _ => None,
        }),
        _ => None,
    };
    if let Some((kind, target)) = link {
        items.push(json!({"kind":"link", "link_type":kind,"target":target,"path":document.path(),"source":document.source(node.location())}));
    }
}

#[cfg(test)]
mod tests;

fn author(value: AuthorRef<'_>) -> Value {
    let mut author = json!({"firstname":value.first_name(), "lastname":value.last_name(), "initials":value.initials()});
    if let Some(name) = value.middle_name() {
        author["middlename"] = json!(name);
    }
    if let Some(address) = value.email() {
        author["address"] = json!(address);
    }
    author
}

fn same_origin(left: PositionRef<'_>, right: PositionRef<'_>) -> bool {
    match (left.include_chain(), right.include_chain()) {
        (Some(left), Some(right)) => left.eq(right),
        (None, None) => true,
        _ => false,
    }
}
