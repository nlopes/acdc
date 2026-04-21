//! Single document state management

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use acdc_parser::{Document, DocumentAttributes, Location};
use tower_lsp_server::ls_types::Diagnostic;

/// Owned counterpart to `acdc_parser::Source<'_>`, detached from the parser arena
/// so it can live in `DocumentState` alongside other extracted index data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OwnedSource {
    Path(PathBuf),
    Url(String),
    Name(String),
}

impl OwnedSource {
    pub(crate) fn from_borrowed(source: &acdc_parser::Source<'_>) -> Self {
        match source {
            acdc_parser::Source::Path(p) => Self::Path(p.clone()),
            acdc_parser::Source::Url(u) => Self::Url(u.to_string()),
            acdc_parser::Source::Name(n) => Self::Name((*n).to_string()),
        }
    }
}

/// Parsed-document wrapper for the LSP state.
///
/// Carries the preprocessed source text in an always-available `Box<str>`
/// (so text-only features keep working even when parsing fails), and
/// optionally the `acdc_parser::ParsedDocument` (arena + AST) when parsing
/// succeeded.
///
/// Because `bumpalo::Bump` is not `Sync`, the `ParsedDocument` is kept
/// behind a [`Mutex`] so `DocumentState` can be stored in a
/// `DashMap<Uri, DocumentState>` without relying on unsafe `Sync` impls.
/// AST access goes through [`Self::ast`] which returns an [`AstGuard`]
/// holding the lock for the duration of the caller's use; the raw text is
/// always accessible via [`Self::text`] without locking.
#[derive(Debug)]
pub(crate) struct ParsedText {
    /// Source text (preprocessed when available, raw otherwise).
    source: Box<str>,
    /// Parsed document, when available. Behind a `Mutex` purely to satisfy
    /// `Sync` — the inner value is only ever read.
    parsed: Option<Mutex<acdc_parser::ParseResult>>,
}

/// RAII guard that keeps a parsed document locked for the duration of read
/// access. Use [`Self::document`] to obtain a reference to the AST.
pub(crate) struct AstGuard<'a> {
    guard: MutexGuard<'a, acdc_parser::ParseResult>,
}

impl AstGuard<'_> {
    /// Borrow the parsed document. The underlying mutex remains locked until
    /// this guard is dropped.
    pub(crate) fn document(&self) -> &Document<'_> {
        self.guard.document()
    }
}

impl ParsedText {
    /// Wrap a successfully parsed document along with the raw (pre-
    /// preprocessor) source text the LSP was given. We keep the raw text
    /// because LSP features (include/conditional scans, formatting,
    /// position-to-offset math) need the text the editor is showing —
    /// the parser's own source has directives already expanded or
    /// consumed. The caller is expected to have already drained warnings
    /// out of the `ParseResult` (via `take_warnings()`) before calling
    /// this, so the stored value carries the AST alone.
    pub(crate) fn from_parsed(raw_source: Box<str>, parsed: acdc_parser::ParseResult) -> Self {
        Self {
            source: raw_source,
            parsed: Some(Mutex::new(parsed)),
        }
    }

    /// Wrap raw source text (used when parsing failed).
    pub(crate) fn from_source(source: Box<str>) -> Self {
        Self {
            source,
            parsed: None,
        }
    }

    /// Borrow the source text without locking.
    pub(crate) fn text(&self) -> &str {
        &self.source
    }

    /// Obtain a locked handle to the parsed AST if parsing succeeded.
    ///
    /// Dereference the returned guard to get `&Document<'_>`. Drops the
    /// underlying mutex when the guard is dropped.
    pub(crate) fn ast(&self) -> Option<AstGuard<'_>> {
        let parsed = self.parsed.as_ref()?;
        let guard = parsed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(AstGuard { guard })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConditionalDirectiveKind {
    Ifdef,
    Ifndef,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConditionalOperation {
    And,
    Or,
}

/// A conditional directive block extracted from raw source text.
///
/// The preprocessor flattens these before the AST is built, so we scan raw text
/// to recover them for editor features (graying out inactive branches).
#[derive(Debug, Clone)]
pub(crate) struct ConditionalBlock {
    pub(crate) kind: ConditionalDirectiveKind,
    pub(crate) attributes: Vec<String>,
    pub(crate) operation: Option<ConditionalOperation>,
    pub(crate) is_active: bool,
    /// 0-indexed line of the opening directive
    pub(crate) start_line: usize,
    /// 0-indexed line of the closing endif (None for single-line form)
    pub(crate) end_line: Option<usize>,
}

/// Represents a parsed document's state.
///
/// The source text and the parsed AST are bound together via [`ParsedText`]
/// because the AST borrows slices of the source string from its arena. Other
/// index data (anchors, xrefs, media sources, etc.) are extracted into owned
/// `String`/`'static` forms so they can be queried independently of the AST.
#[derive(Debug)]
pub(crate) struct DocumentState {
    /// Source text + parsed AST. Access the AST via [`ParsedText::ast`].
    pub(crate) parsed: ParsedText,
    /// Version from the editor (for sync validation)
    pub(crate) version: i32,
    /// Parse errors converted to diagnostics
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Anchor definitions: id -> Location
    pub(crate) anchors: HashMap<String, Location>,
    /// Cross-references: (`target_id`, location)
    pub(crate) xrefs: Vec<(String, Location)>,
    /// Include directives: (`target_path`, location)
    pub(crate) includes: Vec<(String, Location)>,
    /// Attribute references: (`attr_name`, location) extracted from source text
    pub(crate) attribute_refs: Vec<(String, Location)>,
    /// Attribute definitions: (`attr_name`, location) extracted from source text
    pub(crate) attribute_defs: Vec<(String, Location)>,
    /// Media sources: (source, location) for images, audio, and video
    pub(crate) media_sources: Vec<(OwnedSource, Location)>,
    /// Conditional directive blocks (ifdef/ifndef) extracted from source text
    pub(crate) conditionals: Vec<ConditionalBlock>,
}

impl DocumentState {
    /// Borrow the source text.
    #[must_use]
    pub(crate) fn text(&self) -> &str {
        self.parsed.text()
    }

    /// Obtain a locked handle to the parsed AST, if parsing succeeded.
    ///
    /// Dereferences to `&Document<'_>`; the underlying mutex is held for
    /// the guard's lifetime, so keep the guard short-lived.
    pub(crate) fn ast(&self) -> Option<AstGuard<'_>> {
        self.parsed.ast()
    }

    /// Construct a `DocumentState` representing a parse failure.
    ///
    /// Used by tests that need to model "we have text but no AST". The
    /// production failure path lives inline in [`Workspace::parse_and_index`]
    /// where it shares the [`ParsedText`] cell with the success path.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn new_failure(text: String, version: i32, diagnostics: Vec<Diagnostic>) -> Self {
        let parsed = ParsedText::from_source(text.into_boxed_str());
        let raw_text = parsed.text();
        let definitions = extract_attribute_defs(raw_text);
        let references = extract_attribute_refs(raw_text);
        let raw_includes = extract_includes(raw_text);
        let raw_conditionals = extract_conditionals(raw_text, &DocumentAttributes::default());

        Self {
            parsed,
            version,
            diagnostics,
            anchors: HashMap::new(),
            xrefs: vec![],
            includes: raw_includes,
            attribute_refs: references,
            attribute_defs: definitions,
            media_sources: vec![],
            conditionals: raw_conditionals,
        }
    }
}

/// Extract attribute definitions (`:name: value`) from raw text.
pub(crate) fn extract_attribute_defs(text: &str) -> Vec<(String, Location)> {
    let mut defs = Vec::new();
    let mut line_start = 0usize;

    for (line_idx, line) in text.lines().enumerate() {
        let this_line_start = line_start;
        line_start += line.len() + 1;

        let trimmed = line.trim();
        let after_colon = if let Some(rest) = trimmed.strip_prefix(":!") {
            rest
        } else if let Some(rest) = trimmed.strip_prefix(':') {
            rest
        } else {
            continue;
        };

        let Some(end) = after_colon.find(':') else {
            continue;
        };
        if let Some(name_candidate) = after_colon.get(..end)
            && !name_candidate.is_empty()
            && name_candidate
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            let col_offset = line.find(':').unwrap_or(0);
            let line_end = line.len();

            let mut location = Location::default();
            location.start.line = line_idx + 1;
            location.start.column = col_offset + 1;
            location.end.line = line_idx + 1;
            location.end.column = line_end;
            location.absolute_start = this_line_start + col_offset;
            location.absolute_end = this_line_start + line_end;

            defs.push((name_candidate.to_string(), location));
        }
    }

    defs
}

/// Extract attribute references (`{name}`) from raw text.
///
/// Scans for `{name}` patterns, skipping escaped references (`\{name}`)
/// and attribute definition lines (`:name:`).
pub(crate) fn extract_attribute_refs(text: &str) -> Vec<(String, Location)> {
    let mut refs = Vec::new();
    let mut line_start = 0usize;

    for (line_idx, line) in text.lines().enumerate() {
        let this_line_start = line_start;
        line_start += line.len() + 1;

        let trimmed = line.trim();
        // Check if this is an attribute definition: :name: value
        if let Some(after_colon) = trimmed.strip_prefix(':')
            && let Some(end) = after_colon.find(':')
            && let Some(name_candidate) = after_colon.get(..end)
            && !name_candidate.is_empty()
            && name_candidate
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            if let Some(value_part) = after_colon.get(end + 1..) {
                extract_refs_from_line(this_line_start, line, line_idx, value_part, &mut refs);
            }
            continue;
        }
        extract_refs_from_line(this_line_start, line, line_idx, line, &mut refs);
    }

    refs
}

/// Extract `{name}` references from a text segment within a line.
fn extract_refs_from_line(
    line_start: usize,
    line: &str,
    line_idx: usize,
    segment: &str,
    refs: &mut Vec<(String, Location)>,
) {
    // segment is always a substring of line (either line itself or a suffix)
    let segment_offset_in_line = segment.as_ptr() as usize - line.as_ptr() as usize;

    let mut search_start = 0;
    while let Some(open) = segment.get(search_start..).and_then(|s| s.find('{')) {
        let open = search_start + open;

        // Check for escape: \{
        if open > 0 && segment.as_bytes().get(open - 1) == Some(&b'\\') {
            search_start = open + 1;
            continue;
        }

        let Some(close) = segment.get(open + 1..).and_then(|s| s.find('}')) else {
            break;
        };
        let close = open + 1 + close;

        if let Some(name) = segment.get(open + 1..close)
            && !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            let col_in_line = segment_offset_in_line + open;
            let col_end = segment_offset_in_line + close + 1;

            let mut location = Location::default();
            location.start.line = line_idx + 1;
            location.start.column = col_in_line + 1;
            location.end.line = line_idx + 1;
            location.end.column = col_end;
            location.absolute_start = line_start + col_in_line;
            location.absolute_end = line_start + col_end;

            refs.push((name.to_string(), location));
        }

        search_start = close + 1;
    }
}

/// Pending conditional for matching with endif.
struct PendingConditional {
    kind: ConditionalDirectiveKind,
    attributes: Vec<String>,
    operation: Option<ConditionalOperation>,
    is_active: bool,
    start_line: usize,
}

/// Parse attribute names and operation from a conditional directive's attribute part.
///
/// Examples: `"attr"` → (`["attr"]`, None), `"a,b"` → (`["a","b"]`, Some(Or)), `"a+b"` → (`["a","b"]`, Some(And))
fn parse_conditional_attributes(attr_part: &str) -> (Vec<String>, Option<ConditionalOperation>) {
    if let Some((first, rest)) = attr_part.split_once(',') {
        let mut attrs = vec![first.to_string()];
        attrs.extend(rest.split(',').map(String::from));
        (attrs, Some(ConditionalOperation::Or))
    } else if let Some((first, rest)) = attr_part.split_once('+') {
        let mut attrs = vec![first.to_string()];
        attrs.extend(rest.split('+').map(String::from));
        (attrs, Some(ConditionalOperation::And))
    } else {
        (vec![attr_part.to_string()], None)
    }
}

/// Evaluate an ifdef/ifndef condition against document attributes.
fn evaluate_condition(
    attributes: &[String],
    operation: Option<&ConditionalOperation>,
    is_ifndef: bool,
    doc_attrs: &DocumentAttributes,
) -> bool {
    let result = match operation {
        Some(ConditionalOperation::Or) => attributes.iter().any(|a| doc_attrs.contains_key(a)),
        _ => attributes.iter().all(|a| doc_attrs.contains_key(a)),
    };
    if is_ifndef { !result } else { result }
}

/// Extract conditional directive blocks (ifdef/ifndef/endif) from raw text.
///
/// The preprocessor flattens these before the AST is built. We scan raw text
/// to recover them for graying out inactive branches in the editor.
pub(crate) fn extract_conditionals(
    text: &str,
    attrs: &DocumentAttributes,
) -> Vec<ConditionalBlock> {
    let mut blocks = Vec::new();
    let mut pending: Vec<PendingConditional> = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        // Skip escaped directives
        if trimmed.starts_with("\\ifdef")
            || trimmed.starts_with("\\ifndef")
            || trimmed.starts_with("\\ifeval")
        {
            continue;
        }

        // Skip ifeval (deferred — requires expression evaluation)
        if trimmed.starts_with("ifeval::") {
            continue;
        }

        // Check for ifdef:: or ifndef::
        let (rest, is_ifndef) = if let Some(rest) = trimmed.strip_prefix("ifdef::") {
            (rest, false)
        } else if let Some(rest) = trimmed.strip_prefix("ifndef::") {
            (rest, true)
        } else if trimmed.starts_with("endif::") {
            // Close the most recent pending conditional
            if let Some(pending_cond) = pending.pop() {
                blocks.push(ConditionalBlock {
                    kind: pending_cond.kind,
                    attributes: pending_cond.attributes,
                    operation: pending_cond.operation,
                    is_active: pending_cond.is_active,
                    start_line: pending_cond.start_line,
                    end_line: Some(line_idx),
                });
            }
            continue;
        } else {
            continue;
        };

        // Parse: attributes[optional content]
        let Some(bracket_start) = rest.find('[') else {
            continue;
        };
        let attr_part = &rest[..bracket_start];
        if attr_part.is_empty() {
            continue;
        }

        let (attributes, operation) = parse_conditional_attributes(attr_part);
        let kind = if is_ifndef {
            ConditionalDirectiveKind::Ifndef
        } else {
            ConditionalDirectiveKind::Ifdef
        };
        let is_active = evaluate_condition(&attributes, operation.as_ref(), is_ifndef, attrs);

        // Check for single-line form: ifdef::attr[content]
        let bracket_content = rest.get(bracket_start + 1..rest.len().saturating_sub(1));
        if bracket_content.is_some_and(|c| !c.is_empty()) {
            // Single-line form
            blocks.push(ConditionalBlock {
                kind,
                attributes,
                operation,
                is_active,
                start_line: line_idx,
                end_line: None,
            });
        } else {
            // Block form — push to pending stack
            pending.push(PendingConditional {
                kind,
                attributes,
                operation,
                is_active,
                start_line: line_idx,
            });
        }
    }

    blocks
}

/// Extract include directives from raw text via line-by-line scan.
///
/// The preprocessor consumes `include::` directives so they don't appear in the AST.
/// We scan the raw text to find them for document link support, call hierarchy, and
/// file rename operations.
pub(crate) fn extract_includes(text: &str) -> Vec<(String, Location)> {
    let mut includes = Vec::new();
    let mut line_start = 0usize;

    for (line_idx, line) in text.lines().enumerate() {
        let this_line_start = line_start;
        line_start += line.len() + 1;

        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("include::")
            && let Some(bracket_pos) = rest.find('[')
        {
            let target = &rest[..bracket_pos];
            if !target.is_empty() {
                let col_offset = line.find("include::").unwrap_or(0);
                let target_start = col_offset + "include::".len();
                let target_end = target_start + target.len();

                let mut location = Location::default();
                location.start.line = line_idx + 1;
                location.start.column = target_start + 1;
                location.end.line = line_idx + 1;
                location.end.column = target_end;
                location.absolute_start = this_line_start + target_start;
                location.absolute_end = this_line_start + target_end;

                includes.push((target.to_string(), location));
            }
        }
    }

    includes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_includes_basic() {
        let text = "= Document\n\ninclude::chapter1.adoc[]\n\nSome text.\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 1);
        assert_eq!(
            includes.first().map(|(t, _)| t.as_str()),
            Some("chapter1.adoc")
        );
    }

    #[test]
    fn test_extract_includes_with_attributes() {
        let text = "include::partial.adoc[leveloffset=+1]\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 1);
        assert_eq!(
            includes.first().map(|(t, _)| t.as_str()),
            Some("partial.adoc")
        );
    }

    #[test]
    fn test_extract_includes_multiple() {
        let text = "include::a.adoc[]\nSome text\ninclude::b.adoc[]\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 2);
        assert_eq!(includes.first().map(|(t, _)| t.as_str()), Some("a.adoc"));
        assert_eq!(includes.get(1).map(|(t, _)| t.as_str()), Some("b.adoc"));
    }

    #[test]
    fn test_extract_includes_with_path() {
        let text = "include::docs/chapters/intro.adoc[]\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 1);
        assert_eq!(
            includes.first().map(|(t, _)| t.as_str()),
            Some("docs/chapters/intro.adoc")
        );
    }

    #[test]
    fn test_extract_includes_no_includes() {
        let text = "= Document\n\nJust regular text.\n";
        let includes = extract_includes(text);
        assert!(includes.is_empty());
    }

    #[test]
    fn test_extract_includes_location() -> Result<(), Box<dyn std::error::Error>> {
        let text = "= Doc\n\ninclude::file.adoc[]\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 1);
        let (_, loc) = includes.first().ok_or("expected at least one include")?;
        // Line 3 (index 2), 1-indexed = 3
        assert_eq!(loc.start.line, 3);
        Ok(())
    }

    #[test]
    fn test_extract_attribute_refs_basic() {
        let text = "== Section\n\nSee {my-attr} here.\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("my-attr"));
    }

    #[test]
    fn test_extract_attribute_refs_multiple_on_same_line() {
        let text = "The {foo} and {bar} values.\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("foo"));
        assert_eq!(refs.get(1).map(|(n, _)| n.as_str()), Some("bar"));
    }

    #[test]
    fn test_extract_attribute_refs_escaped() {
        let text = "Not a ref: \\{escaped} but {real} is.\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("real"));
    }

    #[test]
    fn test_extract_attribute_refs_skips_definition_name() {
        let text = ":my-attr: some value\n\n{my-attr} is used here.\n";
        let refs = extract_attribute_refs(text);
        // Should find the ref on line 3, not the definition on line 1
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("my-attr"));
        assert_eq!(refs.first().map(|(_, l)| l.start.line), Some(3));
    }

    #[test]
    fn test_extract_attribute_refs_in_definition_value() {
        let text = ":derived: prefix-{base}\n";
        let refs = extract_attribute_refs(text);
        // Should find {base} in the value part of the definition
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("base"));
    }

    #[test]
    fn test_extract_attribute_refs_location() {
        let text = "= Doc\n\n{version} is the version.\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("version"));
        assert_eq!(refs.first().map(|(_, l)| l.start.line), Some(3));
        assert_eq!(refs.first().map(|(_, l)| l.start.column), Some(1));
    }

    #[test]
    fn test_extract_attribute_refs_ignores_invalid_names() {
        let text = "{} and {with spaces} and {valid-name}\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("valid-name"));
    }

    fn attr_set(
        name: &str,
    ) -> (
        std::borrow::Cow<'static, str>,
        acdc_parser::AttributeValue<'static>,
    ) {
        (
            std::borrow::Cow::Owned(name.to_string()),
            acdc_parser::AttributeValue::String(std::borrow::Cow::Owned(String::new())),
        )
    }

    #[test]
    fn test_extract_conditionals_ifdef_active() {
        let mut attrs = DocumentAttributes::default();
        let (k, v) = attr_set("backend-html");
        attrs.insert(k, v);
        let text = ":backend-html:\n\nifdef::backend-html[]\nHTML content\nendif::[]";
        let conds = extract_conditionals(text, &attrs);
        assert_eq!(conds.len(), 1);
        assert_eq!(
            conds.first().map(|c| &c.kind),
            Some(&ConditionalDirectiveKind::Ifdef)
        );
        assert_eq!(
            conds.first().map(|c| c.attributes.as_slice()),
            Some(["backend-html".to_string()].as_slice())
        );
        assert_eq!(conds.first().map(|c| c.is_active), Some(true));
        assert_eq!(conds.first().map(|c| c.start_line), Some(2));
        assert_eq!(conds.first().map(|c| c.end_line), Some(Some(4)));
    }

    #[test]
    fn test_extract_conditionals_ifdef_inactive() {
        let attrs = DocumentAttributes::default();
        let text = "ifdef::backend-html[]\nHTML content\nendif::[]";
        let conds = extract_conditionals(text, &attrs);
        assert_eq!(conds.len(), 1);
        assert_eq!(conds.first().map(|c| c.is_active), Some(false));
        assert_eq!(conds.first().map(|c| c.start_line), Some(0));
        assert_eq!(conds.first().map(|c| c.end_line), Some(Some(2)));
    }

    #[test]
    fn test_extract_conditionals_ifndef() {
        let attrs = DocumentAttributes::default();
        let text = "ifndef::draft[]\nPublished content\nendif::[]";
        let conds = extract_conditionals(text, &attrs);
        assert_eq!(conds.len(), 1);
        assert_eq!(
            conds.first().map(|c| &c.kind),
            Some(&ConditionalDirectiveKind::Ifndef)
        );
        // draft is NOT defined, so ifndef is active
        assert_eq!(conds.first().map(|c| c.is_active), Some(true));
    }

    #[test]
    fn test_extract_conditionals_ifndef_inactive() {
        let mut attrs = DocumentAttributes::default();
        let (k, v) = attr_set("draft");
        attrs.insert(k, v);
        let text = "ifndef::draft[]\nDraft content\nendif::[]";
        let conds = extract_conditionals(text, &attrs);
        assert_eq!(conds.len(), 1);
        // draft IS defined, so ifndef is inactive
        assert_eq!(conds.first().map(|c| c.is_active), Some(false));
    }

    #[test]
    fn test_extract_conditionals_or_operation() {
        let mut attrs = DocumentAttributes::default();
        let (k, v) = attr_set("b");
        attrs.insert(k, v);
        let text = "ifdef::a,b[]\ncontent\nendif::[]";
        let conds = extract_conditionals(text, &attrs);
        assert_eq!(conds.len(), 1);
        assert_eq!(
            conds.first().map(|c| &c.operation),
            Some(&Some(ConditionalOperation::Or))
        );
        // Only b is defined, but OR means any → active
        assert_eq!(conds.first().map(|c| c.is_active), Some(true));
    }

    #[test]
    fn test_extract_conditionals_and_operation() {
        let mut attrs = DocumentAttributes::default();
        let (k, v) = attr_set("a");
        attrs.insert(k, v);
        let text = "ifdef::a+b[]\ncontent\nendif::[]";
        let conds = extract_conditionals(text, &attrs);
        assert_eq!(conds.len(), 1);
        assert_eq!(
            conds.first().map(|c| &c.operation),
            Some(&Some(ConditionalOperation::And))
        );
        // Only a is defined, AND requires both → inactive
        assert_eq!(conds.first().map(|c| c.is_active), Some(false));
    }

    #[test]
    fn test_extract_conditionals_single_line() {
        let attrs = DocumentAttributes::default();
        let text = "ifdef::attr[inline content]";
        let conds = extract_conditionals(text, &attrs);
        assert_eq!(conds.len(), 1);
        assert_eq!(conds.first().map(|c| c.is_active), Some(false));
        assert_eq!(conds.first().map(|c| c.start_line), Some(0));
        assert_eq!(conds.first().map(|c| c.end_line), Some(None));
    }

    #[test]
    fn test_extract_conditionals_escaped() {
        let attrs = DocumentAttributes::default();
        let text = "\\ifdef::attr[]\ncontent\nendif::[]";
        let conds = extract_conditionals(text, &attrs);
        assert!(conds.is_empty());
    }

    #[test]
    fn test_extract_conditionals_multiple() {
        let mut attrs = DocumentAttributes::default();
        let (k, v) = attr_set("html");
        attrs.insert(k, v);
        let text = "ifdef::html[]\nHTML\nendif::[]\nifdef::pdf[]\nPDF\nendif::[]";
        let conds = extract_conditionals(text, &attrs);
        assert_eq!(conds.len(), 2);
        assert_eq!(conds.first().map(|c| c.is_active), Some(true)); // html defined
        assert_eq!(conds.get(1).map(|c| c.is_active), Some(false)); // pdf not defined
    }
}
