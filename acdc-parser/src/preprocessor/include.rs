//! Include processing and source locations.
//!
//! Select content before you process nested directives. Otherwise, an include in
//! excluded text can read a file or produce a warning.
//!
//! # Processing order
//!
//! 1. Parse the directive and select the applicable content selector.
//! 2. Read and decode the source in chunks.
//! 3. Keep the selected lines within the size limit.
//! 4. Apply `indent` to these lines.
//! 5. Process nested directives in the selected `AsciiDoc` text.
//! 6. Add the source and `leveloffset` ranges to the parent include.
//!
//! The reader must read through the source to reach the requested lines or tags.
//! A finite line selection stops at its highest requested line.
//! Full includes, open-ended ranges, and tag selections read to the end of the source.
//!
//! # Content selection
//!
//! [`ContentSelection`] holds the applicable selector.
//! `lines` takes priority over `tag`, and `tag` takes priority over `tags`.
//! Attribute order does not change this priority. If a selector occurs more than
//! once, its last value applies. Ignored selectors cannot produce warnings.
//!
//! Line numbers refer to the original source. Each selected line appears once,
//! in source order. A negative range end means the end of the source.
//!
//! # Tag selection
//!
//! [`Selector`](super::tag::Selector) selects tagged text in one pass.
//! It uses a stack to track nested tags, including tags with the same name.
//! The stack also lets it handle closing markers that do not match the current tag.
//! Each entry stores the tag name, selection state, and opening line number.
//!
//! The tag scanner reports missing tags and invalid tag boundaries as [`TagIssue`]
//! values. It does not know the include directive's location.
//! [`Include::report_tag_issue`] adds that location and creates a [`crate::Warning`].
//!
//! # Source locations
//!
//! Selected lines need their original locations. For example, source lines 6, 8,
//! and 10 become input lines 1, 2, and 3 when we process the selected text.
//! Diagnostics and AST locations must still refer to lines 6, 8, and 10.
//!
//! Each selected line has a [`SourceLineOrigin`]. It stores the original line number,
//! byte offset, and column change from `indent`. The preprocessor uses these values
//! for diagnostics and source ranges.
//!
//! A nested include returns complete source ranges. The parent adjusts their output
//! offsets and adds its target to the start of each include chain.
//! This preserves the locations in nested includes without rebuilding each range.
//!
//! For sources other than `AsciiDoc`, we build source ranges from the same original
//! locations. We do not process nested directives in those sources.
//!
//! # Fallbacks and text without directives
//!
//! Fallback links and unresolved directives refer to the include directive's
//! location. [`IncludeResult::synthetic`] tells the parent to use that location.
//!
//! If the selected `AsciiDoc` text needs no preprocessing, we only build source
//! ranges. This keeps the text unchanged and avoids unnecessary processing.

use std::{
    cell::RefCell,
    collections::HashSet,
    mem::take,
    path::{Component, Path, PathBuf},
    rc::Rc,
    str::FromStr,
};

use url::Url;

use crate::{
    IncludeSourceErrorKind, IncludeSourceProvider, IncludeSourceTarget, Location, Options,
    Preprocessor, SafeMode, Warning,
    error::{Error, SourceLocation},
    model::{HEADER, LeveloffsetRange, Position, SourceRange, substitute},
};

use super::{
    IncludeContext, SourceLineOrigin, SourceOrigin, absolute_normalized,
    tag::{Filter as TagFilter, Issue as TagIssue},
};

mod reader;

/// Maximum number of spaces that a directive can add before each non-empty line.
const MAX_INCLUDE_INDENT: usize = 4 * 1024;

/**
The format of an include directive is the following:

`include::target[leveloffset=offset,lines=ranges,tag(s)=name(s),indent=depth,encoding=encoding,opts=optional]`

The target is required. The target may be an absolute path, a path relative to the
current document, or a URL.

The include directive can be escaped.

If you don't want the include directive to be processed, you must escape it using a
backslash.

`\include::just-an-example.ext[]`

Escaping the directive is necessary even if it appears in a verbatim block since it's
not aware of the surrounding document structure.
*/
#[derive(Debug)]
pub(crate) struct Include<'a> {
    source_origin: SourceOrigin,
    target: Target,
    target_as_written: String,
    level_offset: Option<isize>,
    selection: ContentSelection,
    indent: Option<usize>,
    encoding: Option<String>,
    opts: Vec<String>,
    options: Options<'a>,
    context: IncludeContext,
    // Location information for error reporting
    line_number: usize,
    current_offset: usize,
    current_file: Option<PathBuf>,
    /// Shared warnings sink threaded from the outer `Preprocessor` so
    /// non-fatal include conditions (disabled URL includes, missing
    /// files, bad line numbers) reach `ParseResult::warnings()`.
    warnings: Rc<RefCell<Vec<Warning>>>,
}

/// The one content selector that applies to an include after attribute
/// precedence has been resolved (`lines` > `tag` > `tags`).
#[derive(Debug, PartialEq, Eq)]
enum ContentSelection {
    All,
    Lines(Vec<LinesRange>),
    Tags(Vec<TagFilter>),
}

/// A line range that an include may specify.
///
/// If the range contains `..` then it is a range of lines, if not, it is parsed as a
/// single line.
///
/// There can be multiple of these in an include definition.
#[derive(Debug, PartialEq, Eq)]
enum LinesRange {
    /// A single line
    Single(usize),

    /// A range of lines
    Range(usize, isize),
}

/// The target of the include, which can be a filesystem path pointing to a file, or a
/// url.
///
/// NOTE: URLs will only be fetched if the caller supplied the `allow-uri-read` attribute.
#[derive(Debug)]
pub(crate) enum Target {
    Path(PathBuf),
    Url(String),
    UnsupportedUri(String),
}

impl Target {
    fn parse(target: &str, source_origin: &SourceOrigin) -> Result<Self, Error> {
        if let Some(scheme) = include_uri_scheme(target) {
            if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
                Url::parse(target)?;
                return Ok(Self::Url(target.to_string()));
            }
            return Ok(Self::UnsupportedUri(target.to_string()));
        }

        if let SourceOrigin::Uri(containing_uri) = source_origin {
            let uri = format!("{}/{target}", uri_directory(containing_uri));
            Url::parse(&uri)?;
            return Ok(Self::Url(uri));
        }

        Ok(Self::Path(PathBuf::from(target)))
    }
}

/// Recognize the ASCII scheme syntax from RFC 3986, but require two scheme
/// characters as Asciidoctor does to keep Windows drive paths such as `c:/`
/// local. MRI Asciidoctor's Unicode-aware character classes are a historical
/// regex-engine side effect (its Opal implementation remained ASCII), so acdc
/// intentionally follows the portable scheme syntax instead.
///
/// Provenance: Claude investigated this behavior and wrote this function; this seems fine
/// and I also looked at the asciidoctor source myself and _roughly_ matches. I'm not sure
/// if I'm just carrying dead weight from asciidoctor though.
fn include_uri_scheme(target: &str) -> Option<&str> {
    let (scheme, _) = target.split_once(':')?;
    let (first, rest) = scheme.as_bytes().split_first()?;
    (!rest.is_empty()
        && first.is_ascii_alphabetic()
        && rest
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-')))
    .then_some(scheme)
}

/// Directory portion of a URI without normalizing its path.
///
/// Asciidoctor appends nested targets to this string literally, preserving
/// doubled slashes and `..` segments in the resulting HTTP request.
fn uri_directory(uri: &str) -> &str {
    let path_end = uri.find(['?', '#']).unwrap_or(uri.len());
    let uri_without_suffix = &uri[..path_end];
    let authority_start = uri_without_suffix.find("://").map_or(0, |index| index + 3);
    uri_without_suffix[authority_start..]
        .rfind('/')
        .map_or(uri_without_suffix, |index| {
            &uri_without_suffix[..authority_start + index]
        })
}

/// Location context for error reporting in include directives
#[derive(Debug, Clone, Copy)]
pub(super) struct LocationContext<'a> {
    line_number: usize,
    current_offset: usize,
    current_file: Option<&'a Path>,
}

impl<'a> LocationContext<'a> {
    pub(super) const fn new(
        line_number: usize,
        current_offset: usize,
        current_file: Option<&'a Path>,
    ) -> Self {
        Self {
            line_number,
            current_offset,
            current_file,
        }
    }
}

/// Bundled inputs for the `include_parser` PEG grammar.
///
/// The grammar needs the owning file path, parser options, caller URI authority,
/// location info for diagnostics, and a shared warnings sink. Passing them as one
/// struct keeps each generated rule under clippy's argument-count limit.
struct IncludeParserInputs<'a, 'b> {
    source_origin: &'b SourceOrigin,
    options: &'b Options<'a>,
    context: IncludeContext,
    location: LocationContext<'b>,
    warnings: &'b Rc<RefCell<Vec<Warning>>>,
}

peg::parser! {
    grammar include_parser<'a, 'b>(inputs: &'b IncludeParserInputs<'a, 'b>) for str {
        pub(crate) rule include() -> Result<Include<'a>, Error>
            = "include::" target:target() "[" attrs:attributes()? "]" {
                let target_raw = substitute(&target, HEADER, &inputs.options.document_attributes);
                let target_as_written = target_raw.into_owned();
                let target = Target::parse(&target_as_written, inputs.source_origin)?;

                let mut include = Include {
                    source_origin: inputs.source_origin.clone(),
                    target,
                    target_as_written,
                    level_offset: None,
                    selection: ContentSelection::All,
                    indent: None,
                    encoding: None,
                    opts: Vec::new(),
                    options: inputs.options.clone(),
                    context: inputs.context.clone(),
                    line_number: inputs.location.line_number,
                    current_offset: inputs.location.current_offset,
                    current_file: inputs.location.current_file.map(Path::to_path_buf),
                    warnings: Rc::clone(inputs.warnings),
                };
                if let Some(attrs) = attrs {
                    include.parse_attributes(attrs)?;
                }
                Ok(include)
            }

        rule target() -> String
            = t:$((!['['] [_])+)
            {?
                if t == t.trim_ascii() {
                    Ok(t.to_string())
                } else {
                    Err("include target without leading or trailing whitespace")
                }
            }

        rule attributes() -> Vec<(String, String)>
            = pair:attribute_pair() pairs:("," p:attribute_pair() { p })* {
                let mut attrs = vec![pair];
                attrs.extend(pairs);
                attrs
            }

        rule attribute_pair() -> (String, String)
            = k:attribute_key() "=" v:attribute_value() {
                (k, v)
            }

        rule attribute_key() -> String
            // Note: "tags" must come before "tag" due to PEG's ordered choice
            = k:$("leveloffset" / "lines" / "tags" / "tag" / "indent" / "encoding" / "opts") {
                k.to_string()
            }

        rule attribute_value() -> String
            = "\"" v:$((!['"'] [_])*) "\"" { v.to_string() }
        / v:$((![','] ![']'] [_])*) { v.to_string() }
    }
}

impl FromStr for LinesRange {
    type Err = Error;

    fn from_str(line_range: &str) -> Result<Self, Self::Err> {
        // FromStr trait implementation for backward compatibility.
        // Prefer using LinesRange::parse() with location info for better error messages.
        Self::from_str_with_location(line_range, None)
    }
}

impl LinesRange {
    /// Helper to create error with location information
    fn create_error(line_range: &str, location: Option<(usize, usize, Option<&Path>)>) -> Error {
        let (line_number, _current_offset, current_file) = location.unwrap_or((1, 0, None));
        Error::InvalidLineRange(
            Box::new(SourceLocation {
                file: current_file.map(Path::to_path_buf),
                location: Location::point(Position::from_line_col(line_number, 1)),
            }),
            line_range.to_string(),
        )
    }

    /// Parse a single line range string with optional location info.
    fn from_str_with_location(
        line_range: &str,
        location: Option<(usize, usize, Option<&Path>)>,
    ) -> Result<Self, Error> {
        if line_range.contains("..") {
            let mut parts = line_range.split("..");
            let start = parts
                .next()
                .ok_or_else(|| Self::create_error(line_range, location))?
                .parse()
                .map_err(|_| Self::create_error(line_range, location))?;
            let end = parts
                .next()
                .ok_or_else(|| Self::create_error(line_range, location))?
                .parse()
                .map_err(|_| Self::create_error(line_range, location))?;
            Ok(LinesRange::Range(start, end))
        } else {
            Ok(LinesRange::Single(line_range.parse().map_err(|e| {
                tracing::error!(?line_range, ?e, "Failed to parse line range");
                Self::create_error(line_range, location)
            })?))
        }
    }

    /// Parse line ranges (possibly multiple, separated by `;` or `,`) with location info.
    fn parse(
        value: &str,
        line_number: usize,
        current_offset: usize,
        current_file: Option<&Path>,
    ) -> Result<Vec<Self>, Error> {
        let location = Some((line_number, current_offset, current_file));

        let separator = if value.contains(';') {
            ';'
        } else if value.contains(',') {
            ','
        } else {
            // Single range, no separator
            return Ok(vec![Self::from_str_with_location(value, location)?]);
        };

        value
            .split(separator)
            .map(|part| Self::from_str_with_location(part, location))
            .collect()
    }
}

/// Result of processing an include directive.
///
/// Contains included text and its source and leveloffset mappings.
#[derive(Debug)]
pub(crate) struct IncludeResult {
    pub(crate) content: String,
    /// Whether `content` is synthesized fallback text that belongs to the include
    /// directive rather than content read from the target.
    pub(crate) synthetic: bool,
    /// The effective leveloffset value to apply to this included content.
    /// This is the sum of the current document's leveloffset and the include's leveloffset.
    pub(crate) effective_leveloffset: Option<isize>,
    /// Leveloffset ranges produced while preprocessing the selected target lines.
    pub(crate) leveloffset_ranges: Vec<LeveloffsetRange>,
    /// The include target exactly as written in the directive (after attribute
    /// substitution), e.g. `markup.adoc` or `chapters/intro.adoc`. Used as the
    /// outermost element of the ASG `file` include chain. Empty when no target
    /// resolved (missing/optional include).
    pub(crate) target: String,
    /// Complete source ranges for the selected target and anything it included,
    /// relative to the beginning of `content`.
    pub(crate) source_ranges: Vec<SourceRange>,
    pub(crate) included_files: HashSet<String>,
    pub(crate) document_attributes: Option<crate::DocumentAttributes<'static>>,
    pub(super) comment_context: Option<super::comment::BlockContext>,
}

enum UrlReadError {
    Unsupported(String),
    Retrieval(String),
    Other(Error),
}

enum UrlIncludeOutcome {
    Content(reader::Selected),
    Fallback(Box<IncludeResult>),
}

impl From<Error> for UrlReadError {
    fn from(error: Error) -> Self {
        Self::Other(error)
    }
}

impl IncludeResult {
    fn empty() -> Self {
        Self {
            content: String::new(),
            synthetic: false,
            effective_leveloffset: None,
            leveloffset_ranges: Vec::new(),
            target: String::new(),
            source_ranges: Vec::new(),
            included_files: HashSet::new(),
            document_attributes: None,
            comment_context: None,
        }
    }

    fn link_fallback(target: &str, compat_mode: bool) -> Self {
        let target = if target.contains(' ') {
            format!("pass:c[{target}]")
        } else {
            target.to_string()
        };
        let attributes = if compat_mode { "" } else { "role=include" };
        Self {
            content: format!("link:{target}[{attributes}]"),
            synthetic: true,
            effective_leveloffset: None,
            leveloffset_ranges: Vec::new(),
            target: String::new(),
            source_ranges: Vec::new(),
            included_files: HashSet::new(),
            document_attributes: None,
            comment_context: None,
        }
    }

    fn unresolved_directive(directive: String) -> Self {
        Self {
            content: directive,
            synthetic: true,
            effective_leveloffset: None,
            leveloffset_ranges: Vec::new(),
            target: String::new(),
            source_ranges: Vec::new(),
            included_files: HashSet::new(),
            document_attributes: None,
            comment_context: None,
        }
    }
}

impl<'a> Include<'a> {
    fn resolve_content_selection(
        line_ranges: Option<Vec<LinesRange>>,
        tag: Option<String>,
        tags: Option<String>,
    ) -> ContentSelection {
        if let Some(ranges) = line_ranges {
            return ContentSelection::Lines(ranges);
        }
        if let Some(value) = tag {
            return match TagFilter::parse(&value) {
                Some(filter) => ContentSelection::Tags(vec![filter]),
                None => ContentSelection::All,
            };
        }
        let Some(value) = tags else {
            return ContentSelection::All;
        };

        let delimiter = if value.contains(',') { ',' } else { ';' };
        let filters = value
            .split(delimiter)
            .filter_map(TagFilter::parse)
            .collect::<Vec<_>>();
        if filters.is_empty() {
            ContentSelection::All
        } else {
            ContentSelection::Tags(filters)
        }
    }

    fn parse_attributes(&mut self, attributes: Vec<(String, String)>) -> Result<(), Error> {
        let mut line_ranges = None;
        let mut tag = None;
        let mut tags = None;

        for (key, value) in attributes {
            match key.as_ref() {
                "leveloffset" => {
                    self.level_offset = Some(value.parse().map_err(|_| {
                        Error::InvalidLevelOffset(
                            Box::new(SourceLocation {
                                file: self.current_file.clone(),
                                location: Location::point(Position::from_line_col(
                                    self.line_number,
                                    1,
                                )),
                            }),
                            value.clone(),
                        )
                    })?);
                }
                "lines" => {
                    line_ranges = Some(LinesRange::parse(
                        &value,
                        self.line_number,
                        self.current_offset,
                        self.current_file.as_deref(),
                    )?);
                }
                "tag" => tag = Some(value),
                "tags" => {
                    tags = Some(value);
                }
                "indent" => {
                    let indent = value.parse().map_err(|_| {
                        Error::InvalidIndent(
                            Box::new(SourceLocation {
                                file: self.current_file.clone(),
                                location: Location::point(Position::from_line_col(
                                    self.line_number,
                                    1,
                                )),
                            }),
                            value.clone(),
                        )
                    })?;
                    if indent > MAX_INCLUDE_INDENT {
                        return Err(Error::IncludeIndentTooLarge(
                            Box::new(SourceLocation {
                                file: self.current_file.clone(),
                                location: Location::point(Position::from_line_col(
                                    self.line_number,
                                    1,
                                )),
                            }),
                            indent,
                            MAX_INCLUDE_INDENT,
                        ));
                    }
                    self.indent = Some(indent);
                }
                "encoding" => {
                    self.encoding = Some(value.clone());
                }
                "opts" => {
                    self.opts.extend(value.split(',').map(str::to_string));
                }
                unknown => {
                    tracing::error!(?unknown, "unknown attribute key in include directive");
                    return Err(Error::InvalidIncludeDirective(
                        Box::new(SourceLocation {
                            file: self.current_file.clone(),
                            location: Location::point(Position::from_line_col(self.line_number, 1)),
                        }),
                        unknown.to_string(),
                    ));
                }
            }
        }

        self.selection = Self::resolve_content_selection(line_ranges, tag, tags);

        Ok(())
    }

    pub(crate) fn parse(
        source_origin: &SourceOrigin,
        line: &str,
        location: LocationContext<'_>,
        options: &Options<'a>,
        include_context: IncludeContext,
        warnings: &Rc<RefCell<Vec<Warning>>>,
    ) -> Result<Self, Error> {
        let inputs = IncludeParserInputs {
            source_origin,
            options,
            context: include_context,
            location,
            warnings,
        };
        include_parser::include(line, &inputs).map_err(|e| {
            tracing::error!(?line, error=?e, "failed to parse include directive");
            let peg_location = e.location;
            Error::Parse(
                Box::new(crate::SourceLocation {
                    file: inputs.location.current_file.map(Path::to_path_buf),
                    // Adjust line number to be relative to the document
                    // PEG parser location.line is always 1 for a single line parse
                    location: Location::point(Position::from_line_col(
                        inputs.location.line_number,
                        peg_location.column,
                    )),
                }),
                e.expected.to_string(),
            )
        })?
    }

    /// The include target exactly as written in the directive (after attribute
    /// substitution), e.g. `markup.adoc` or `chapters/intro.adoc` — not resolved
    /// against the including file's directory. Feeds the ASG `file` include chain.
    fn target_as_written(&self) -> &str {
        &self.target_as_written
    }

    /// Build Asciidoctor's visible recovery line for a failed include read.
    fn unresolved_directive(&self, attribute_list_as_written: &str) -> IncludeResult {
        let target_as_written = self.target_as_written();
        let source = match &self.source_origin {
            SourceOrigin::File {
                path,
                base_dir,
                is_entry,
                ..
            } => {
                let absolute_path = absolute_normalized(path).unwrap_or_else(|_| path.clone());
                let display_path = if *is_entry {
                    absolute_path
                        .file_name()
                        .map_or(absolute_path.as_path(), Path::new)
                } else {
                    absolute_path
                        .strip_prefix(base_dir)
                        .unwrap_or(&absolute_path)
                };
                display_path.display().to_string()
            }
            SourceOrigin::Memory { .. } => "<stdin>".to_string(),
            SourceOrigin::Uri(uri) => uri.clone(),
        };
        IncludeResult::unresolved_directive(format!(
            "Unresolved directive in {source} - include::{target_as_written}[{attribute_list_as_written}]"
        ))
    }

    fn report_tag_issue(&self, issue: TagIssue, resolved_source: &Path) {
        let target_type = match &self.target {
            Target::Path(_) => "file",
            Target::Url(_) | Target::UnsupportedUri(_) => "uri",
        };
        let target = resolved_source.display();
        let message = match issue {
            TagIssue::UnexpectedEnd { name, line } => {
                format!(
                    "unexpected end tag '{name}' at line {line} of include {target_type}: {target}"
                )
            }
            TagIssue::MismatchedEnd {
                expected,
                found,
                line,
            } => format!(
                "mismatched end tag (expected '{expected}' but found '{found}') at line {line} of include {target_type}: {target}"
            ),
            TagIssue::Unclosed { name, line } => format!(
                "detected unclosed tag '{name}' starting at line {line} of include {target_type}: {target}"
            ),
            TagIssue::Missing { names } => {
                let noun = if names.len() == 1 { "tag" } else { "tags" };
                format!(
                    "{noun} '{}' not found in include {target_type}: {target}",
                    names.join(", ")
                )
            }
        };
        self.warn_located(message);
    }

    /// Remove common leading whitespace and add `indent` spaces to nonblank lines.
    /// Return the text and its column change. Return `None` if the text exceeds the size limit.
    fn apply_indent(lines: &[&str], indent: usize) -> Option<(String, isize)> {
        let min_indent = lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);
        let length = lines
            .iter()
            .try_fold(lines.len().saturating_sub(1), |length, line| {
                let extra = if line.trim().is_empty() {
                    0
                } else {
                    line.len() - min_indent + indent
                };
                length
                    .checked_add(extra)
                    .filter(|length| *length <= reader::MAX_SELECTED_BYTES)
            })?;
        let prefix = " ".repeat(indent);
        let mut content = String::with_capacity(length);
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                content.push('\n');
            }
            if !line.trim().is_empty() {
                content.push_str(&prefix);
                content.push_str(&line[min_indent..]);
            }
        }
        let column_shift =
            isize::try_from(indent).unwrap_or(0) - isize::try_from(min_indent).unwrap_or(0);
        Some((content, column_shift))
    }

    fn has_asciidoc_extension(path: &Path) -> bool {
        path.extension().is_some_and(|extension| {
            ["adoc", "asciidoc", "ad", "asc", "txt"].contains(&extension.to_string_lossy().as_ref())
        })
    }

    fn rebase_absolute_target(base_dir: &Path, target: &Path) -> PathBuf {
        let mut recovered = base_dir.to_path_buf();
        for component in target.components() {
            if let Component::Normal(segment) = component {
                recovered.push(segment);
            }
        }
        recovered
    }

    /// Resolve a local target using the Safe/Server entry-directory boundary.
    ///
    /// With `base_dir` set to `/workspace/docs`, `../shared.adoc` becomes
    /// `/workspace/docs/shared.adoc`, and `/tmp/shared.adoc` becomes
    /// `/workspace/docs/tmp/shared.adoc`. Symlinks are deliberately not canonicalized.
    fn resolve_file_target(
        &self,
        current_parent: &Path,
        base_dir: &Path,
        target: &Path,
    ) -> Result<PathBuf, Error> {
        if self.options.safe_mode < SafeMode::Safe {
            return Ok(current_parent.join(target));
        }

        if target.is_absolute() {
            let target = absolute_normalized(target)?;
            if target.starts_with(base_dir) {
                return Ok(target);
            }
            self.warn_located("include file is outside of jail; recovering automatically");
            return Ok(Self::rebase_absolute_target(base_dir, &target));
        }

        let mut resolved = absolute_normalized(current_parent)?;
        if !resolved.starts_with(base_dir) {
            self.warn_located("include file is outside of jail; recovering automatically");
            return Ok(Self::rebase_absolute_target(base_dir, target));
        }

        let mut recovered = false;
        for component in target.components() {
            match component {
                Component::Prefix(_) | Component::RootDir => resolved.push(component.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => {
                    if resolved == base_dir {
                        recovered = true;
                    } else {
                        resolved.pop();
                    }
                }
                Component::Normal(segment) => resolved.push(segment),
            }
        }
        if recovered {
            self.warn_located(
                "include file has illegal reference to ancestor of jail; recovering automatically",
            );
        }
        Ok(resolved)
    }

    /// Apply `indent`, then process nested directives in the selected text.
    fn process_selected_content(
        &self,
        selected: reader::Selected,
        source_origin: &SourceOrigin,
        resolved_source: &Path,
        is_asciidoc: bool,
    ) -> Result<IncludeResult, Error> {
        for issue in selected.issues {
            self.report_tag_issue(issue, resolved_source);
        }
        if let ContentSelection::Lines(ranges) = &self.selection {
            for range in ranges {
                let (LinesRange::Single(start) | LinesRange::Range(start, _)) = range;
                self.validate_line_number(*start);
            }
        }
        let mut selected_lines = if selected.origins.is_empty() {
            Vec::new()
        } else {
            selected.text.split('\n').collect::<Vec<_>>()
        };
        let mut line_origins = selected.origins;
        if is_asciidoc
            && self
                .options
                .document_attributes
                .contains_key("skip-front-matter")
            && let Some(front_matter) = super::skim_front_matter(selected_lines.iter().copied())
        {
            selected_lines.drain(..front_matter.lines_consumed);
            line_origins.drain(..front_matter.lines_consumed);
        }
        let (selected_content, column_shift) = if let Some(indent) = self.indent {
            Self::apply_indent(&selected_lines, indent).ok_or_else(|| {
                Error::IncludeSourceTooLarge(resolved_source.display().to_string())
            })?
        } else {
            (selected_lines.join("\n"), 0)
        };
        for origin in &mut line_origins {
            origin.column_shift = column_shift;
        }
        let mut included = IncludeResult {
            content: selected_content,
            synthetic: false,
            effective_leveloffset: self.calculate_effective_leveloffset(),
            leveloffset_ranges: Vec::new(),
            target: self.target_as_written().to_string(),
            source_ranges: Vec::new(),
            included_files: HashSet::new(),
            document_attributes: None,
            comment_context: None,
        };
        if is_asciidoc {
            let preprocessed = Preprocessor::nested(&self.warnings, self.context.clone())
                .process_mapped(take(&mut included.content), source_origin, &self.options, line_origins)
                .map_err(|error| {
                    tracing::error!(origin=?source_origin, ?error, "failed to process included content");
                    error
                })?;
            included.content = preprocessed.result.text.into_owned();
            included.leveloffset_ranges = preprocessed.result.leveloffset_ranges;
            included.source_ranges = preprocessed.result.source_ranges;
            included.included_files = preprocessed.result.included_files;
            included.document_attributes = Some(preprocessed.document_attributes);
            included.comment_context = Some(preprocessed.result.comment_context);
            let full = match &self.selection {
                ContentSelection::All => true,
                ContentSelection::Lines(_) => false,
                ContentSelection::Tags(filters) => super::tag::selects_all(filters),
            };
            if full
                && !self.opts.iter().any(|option| option == "partial")
                && let Some(name) = source_origin.include_name()
            {
                included.included_files.insert(name);
            }
        } else {
            included.source_ranges =
                Self::source_ranges_for_lines(&included.content, &line_origins, source_origin);
        }
        Ok(included)
    }

    fn source_ranges_for_lines(
        content: &str,
        origins: &[SourceLineOrigin],
        source_origin: &SourceOrigin,
    ) -> Vec<SourceRange> {
        let mut ranges: Vec<SourceRange> = Vec::new();
        let mut cursor = 0;
        let mut expected_line = 0;

        for (line, origin) in content.split('\n').zip(origins) {
            let end_offset = cursor + line.len() + 1;
            if origin.line == expected_line
                && ranges
                    .last()
                    .is_some_and(|range| range.column_shift == origin.column_shift)
            {
                if let Some(range) = ranges.last_mut() {
                    range.end_offset = end_offset;
                }
            } else {
                ranges.push(SourceRange {
                    start_offset: cursor,
                    end_offset,
                    file: source_origin.as_path().map(Path::to_path_buf),
                    file_chain: Vec::new(),
                    start_line: origin.line,
                    source_start_offset: origin.offset,
                    column_shift: origin.column_shift,
                });
            }
            cursor = end_offset;
            expected_line = origin.line + 1;
        }
        ranges
    }

    /// Read and select URI content. Do not process nested directives yet.
    fn read_content_from_url(
        &self,
        url: &str,
        source_provider: &dyn IncludeSourceProvider,
    ) -> Result<reader::Selected, UrlReadError> {
        let source = source_provider
            .open(&IncludeSourceTarget::Uri(url.to_string()))
            .map_err(|error| match error.kind() {
                IncludeSourceErrorKind::Unsupported => UrlReadError::Unsupported(error.to_string()),
                IncludeSourceErrorKind::NotFound | IncludeSourceErrorKind::Unavailable => {
                    UrlReadError::Retrieval(error.to_string())
                }
                IncludeSourceErrorKind::Fatal => UrlReadError::Other(Error::IncludeSource(error)),
            })?;
        reader::read(source, self.encoding.as_deref(), url, &self.selection).map_err(|error| {
            // Discard partial content when the response cannot be read.
            if let Error::Io(error) = error {
                UrlReadError::Retrieval(error.to_string())
            } else {
                UrlReadError::Other(error)
            }
        })
    }

    fn read_url_content_or_fallback(
        &self,
        url: &str,
        attribute_list_as_written: &str,
        source_provider: &dyn IncludeSourceProvider,
    ) -> Result<UrlIncludeOutcome, Error> {
        if !self.context.allows_uri_read {
            self.warn_located(format!(
                "include uri not read because URI access is disabled: {url}"
            ));
            return Ok(UrlIncludeOutcome::Fallback(Box::new(
                IncludeResult::link_fallback(
                    self.target_as_written(),
                    self.options.document_attributes.contains_key("compat-mode"),
                ),
            )));
        }

        match self.read_content_from_url(url, source_provider) {
            Ok(content) => Ok(UrlIncludeOutcome::Content(content)),
            Err(UrlReadError::Unsupported(detail)) => {
                self.warn_located(format!("{detail}, cannot fetch remote includes: {url}"));
                Ok(UrlIncludeOutcome::Fallback(Box::new(
                    self.unresolved_directive(attribute_list_as_written),
                )))
            }
            Err(UrlReadError::Retrieval(detail)) => {
                tracing::debug!(%url, %detail, "failed to retrieve remote include");
                self.warn_located(format!("include uri not readable: {url}"));
                Ok(UrlIncludeOutcome::Fallback(Box::new(
                    self.unresolved_directive(attribute_list_as_written),
                )))
            }
            Err(UrlReadError::Other(error)) => Err(error),
        }
    }

    /// Read and select text from the resolved local file. Do not preprocess it yet.
    fn read_local_content(
        &self,
        path: &Path,
        optional: bool,
        source_provider: &dyn IncludeSourceProvider,
    ) -> Result<Option<reader::Selected>, Error> {
        let source = match source_provider.open(&IncludeSourceTarget::File(path.to_path_buf())) {
            Ok(source) => source,
            Err(error) if error.kind() == IncludeSourceErrorKind::NotFound => {
                if optional {
                    tracing::info!(
                        source_file = ?self.current_file,
                        line = self.line_number,
                        include_path = %path.display(),
                        "optional include dropped because include file not found",
                    );
                } else {
                    self.warn_located(format!("include file not found: {}", path.display()));
                }
                return Ok(None);
            }
            Err(error)
                if !optional
                    && matches!(
                        error.kind(),
                        IncludeSourceErrorKind::Unavailable | IncludeSourceErrorKind::Unsupported
                    ) =>
            {
                self.warn_located(format!("include file not readable: {}", path.display()));
                return Ok(None);
            }
            Err(error) => return Err(Error::IncludeSource(error)),
        };

        let display_path = path.display().to_string();
        let decoded = match reader::read(
            source,
            self.encoding.as_deref(),
            &display_path,
            &self.selection,
        ) {
            Ok(content) => content,
            Err(Error::Io(error)) => {
                if optional {
                    return Err(Error::Io(error));
                }
                self.warn_located(format!("include file not readable: {}", path.display()));
                return Ok(None);
            }
            Err(Error::UnrecognizedEncodingInFile(_))
                if self.encoding.as_deref().is_some_and(|label| {
                    encoding_rs::Encoding::for_label(label.as_bytes())
                        .is_some_and(|encoding| encoding != encoding_rs::UTF_8)
                }) =>
            {
                self.warn_located(format!("include file not readable: {}", path.display()));
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        Ok(Some(decoded))
    }

    pub(crate) fn process(
        &self,
        attribute_list_as_written: &str,
        source_provider: &dyn IncludeSourceProvider,
    ) -> Result<IncludeResult, Error> {
        if self.options.safe_mode == SafeMode::Secure {
            self.warn_located(format!(
                "include not read in secure mode: {}",
                self.target_as_written()
            ));
            return Ok(IncludeResult::link_fallback(
                self.target_as_written(),
                self.options.document_attributes.contains_key("compat-mode"),
            ));
        }

        let (content, source_origin, resolved_source, is_asciidoc) = match &self.target {
            Target::Path(target) => {
                let memory_base;
                let (parent, base_dir) = match &self.source_origin {
                    SourceOrigin::File {
                        path,
                        base_dir,
                        is_entry,
                    } => {
                        let parent = if *is_entry {
                            base_dir.as_path()
                        } else {
                            path.parent().unwrap_or(path)
                        };
                        (parent, base_dir.as_path())
                    }
                    SourceOrigin::Memory { base_dir } => {
                        memory_base = absolute_normalized(
                            base_dir.as_deref().unwrap_or_else(|| Path::new(".")),
                        )?;
                        (memory_base.as_path(), memory_base.as_path())
                    }
                    SourceOrigin::Uri(_) => {
                        self.warn_located(format!(
                            "local include target has a URI source origin: {}",
                            target.display()
                        ));
                        return Ok(IncludeResult::empty());
                    }
                };
                let path = self.resolve_file_target(parent, base_dir, target)?;
                let optional = self.opts.iter().any(|option| option == "optional");
                let Some(content) = self.read_local_content(&path, optional, source_provider)?
                else {
                    return Ok(if optional {
                        IncludeResult::empty()
                    } else {
                        self.unresolved_directive(attribute_list_as_written)
                    });
                };
                let is_asciidoc = Self::has_asciidoc_extension(&path);
                let source_origin = SourceOrigin::File {
                    path: path.clone(),
                    base_dir: base_dir.to_path_buf(),
                    is_entry: false,
                };
                (content, source_origin, path, is_asciidoc)
            }
            Target::Url(url) => {
                let content = match self.read_url_content_or_fallback(
                    url,
                    attribute_list_as_written,
                    source_provider,
                )? {
                    UrlIncludeOutcome::Content(content) => content,
                    UrlIncludeOutcome::Fallback(result) => return Ok(*result),
                };
                let parsed_url = Url::parse(url)?;
                let is_asciidoc = Self::has_asciidoc_extension(Path::new(parsed_url.path()));
                (
                    content,
                    SourceOrigin::Uri(url.clone()),
                    PathBuf::from(url),
                    is_asciidoc,
                )
            }
            Target::UnsupportedUri(uri) => {
                if !self.context.allows_uri_read {
                    self.warn_located(format!(
                        "include uri not read because URI access is disabled: {uri}"
                    ));
                    return Ok(IncludeResult::link_fallback(
                        self.target_as_written(),
                        self.options.document_attributes.contains_key("compat-mode"),
                    ));
                }
                self.warn_located(format!("include uri not readable: {uri}"));
                return Ok(self.unresolved_directive(attribute_list_as_written));
            }
        };
        self.process_selected_content(content, &source_origin, &resolved_source, is_asciidoc)
    }

    /// Calculate the effective leveloffset for this include.
    /// This is the sum of the current document's leveloffset and the include's leveloffset.
    fn calculate_effective_leveloffset(&self) -> Option<isize> {
        self.level_offset.map(|level_offset| {
            let current_offset = self
                .options
                .document_attributes
                .text("leveloffset")
                .map(crate::strip_quotes)
                .and_then(|s| s.parse::<isize>().ok())
                .unwrap_or(0);

            current_offset + level_offset
        })
    }

    fn validate_line_number(&self, num: usize) -> Option<usize> {
        if num < 1 {
            self.warn_located(format!("invalid line number in include directive: {num}"));
            None
        } else {
            Some(num - 1)
        }
    }

    /// Push a warning with the include-directive source location
    /// attached (line from `self.line_number`, column 1 — the preprocessor
    /// operates line-by-line).
    fn warn_located(&self, message: impl Into<std::borrow::Cow<'static, str>>) {
        let source_location = crate::SourceLocation {
            file: self.current_file.clone(),
            location: Location::point(crate::Position::from_line_col(self.line_number, 1)),
        };
        let warning = Warning::new(
            crate::WarningKind::ContentRecovery {
                message: message.into(),
            },
            Some(source_location),
        );
        tracing::warn!("{warning}");
        self.warnings.borrow_mut().push(warning);
    }

    #[cfg(test)]
    fn resolve_end_line(end: isize, max_size: usize) -> Option<usize> {
        match end {
            n if n < 0 => max_size.checked_sub(1),
            n if n > 0 => match usize::try_from(n - 1) {
                Ok(val) => Some(val),
                Err(e) => {
                    tracing::error!(?end, ?e, "failed to cast end line number to usize");
                    None
                }
            },
            _ => {
                tracing::error!(?end, "invalid end line number in include directive");
                None
            }
        }
    }

    /// Collects all line indices that would be selected by the line ranges.
    #[cfg(test)]
    fn collect_line_range_indices(
        &self,
        ranges: &[LinesRange],
        content_lines_count: usize,
    ) -> std::collections::HashSet<usize> {
        let mut indices = std::collections::HashSet::new();
        for line in ranges {
            match line {
                LinesRange::Single(line_number) => {
                    if let Some(idx) = self.validate_line_number(*line_number)
                        && idx < content_lines_count
                    {
                        indices.insert(idx);
                    }
                }
                LinesRange::Range(start, end) => {
                    let Some(start_idx) = self.validate_line_number(*start) else {
                        continue;
                    };
                    let Some(end_idx) = Self::resolve_end_line(*end, content_lines_count) else {
                        continue;
                    };

                    if start_idx < content_lines_count
                        && end_idx < content_lines_count
                        && start_idx <= end_idx
                    {
                        for i in start_idx..=end_idx {
                            indices.insert(i);
                        }
                    }
                }
            }
        }
        indices
    }

    /// Byte offset of each line's first byte within the file's normalized content
    /// (lines joined with a single `\n`), so a surviving line can carry its true
    /// origin-file byte offset.
    #[cfg(test)]
    fn line_start_offsets(content_lines: &[&str]) -> Vec<usize> {
        let mut starts = Vec::with_capacity(content_lines.len());
        let mut offset = 0;
        for line in content_lines {
            starts.push(offset);
            offset += line.len() + 1;
        }
        starts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    pub(super) fn parse_include<'a>(
        path: &Path,
        line: &str,
        options: &Options<'a>,
    ) -> Result<Include<'a>, Error> {
        let source_origin = SourceOrigin::File {
            path: path.join("source.adoc"),
            base_dir: path.to_path_buf(),
            is_entry: true,
        };
        Include::parse(
            &source_origin,
            line,
            LocationContext::new(1, 0, None),
            options,
            IncludeContext::root(options),
            &Rc::default(),
        )
    }

    fn tag_selection(values: &[&str]) -> ContentSelection {
        ContentSelection::Tags(
            values
                .iter()
                .filter_map(|value| TagFilter::parse(value))
                .collect(),
        )
    }

    #[test]
    fn include_uri_scheme_uses_portable_syntax_and_windows_carveout() {
        for (target, expected) in [
            ("ftp://example.test/part.adoc", "ftp"),
            ("data:text/plain,x", "data"),
            ("git+ssh://example.test/path", "git+ssh"),
            ("ab:/path", "ab"),
            ("HTTP://example.test/path", "HTTP"),
        ] {
            assert_eq!(include_uri_scheme(target), Some(expected), "{target}");
        }

        for target in [
            "a:/path",
            "c:/sample.adoc",
            r"c:\sample.adoc",
            "foo_bar:/path",
            "1abc:/path",
            "éx:valeur",
            "ab٣:valeur",
            "ab¾:valeur",
            "ab①:valeur",
        ] {
            assert_eq!(include_uri_scheme(target), None, "{target}");
        }

        let source_origin = SourceOrigin::File {
            path: PathBuf::from("source.adoc"),
            base_dir: PathBuf::new(),
            is_entry: true,
        };
        assert!(matches!(
            Target::parse("HTTP://example.test/path", &source_origin),
            Ok(Target::Url(url)) if url == "HTTP://example.test/path"
        ));
    }

    #[test]
    fn test_parse_simple_include() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[]";
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert!(matches!(
            include.target,
            Target::Path(ref path) if path.as_path() == Path::new("target.adoc")
        ));
        Ok(())
    }

    #[test]
    fn test_parse_include_with_attributes() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[leveloffset=+1,lines=1..5,tag=example]";
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert_eq!(include.level_offset, Some(1));
        assert_eq!(
            include.selection,
            ContentSelection::Lines(vec![LinesRange::Range(1, 5)])
        );
        Ok(())
    }

    #[test]
    fn test_parse_include_with_url() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::https://example.com/doc.adoc[]";
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert!(matches!(
            include.target,
            Target::Url(url) if url.as_str() == "https://example.com/doc.adoc"
        ));
        Ok(())
    }

    #[test]
    fn test_parse_quoted_attributes() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = r#"include::target.adoc[tag="example code",encoding="utf-8"]"#;
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert_eq!(include.selection, tag_selection(&["example code"]));
        assert_eq!(include.encoding, Some("utf-8".to_string()));
        Ok(())
    }

    #[test]
    fn test_parse_include_with_tags_attribute() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[tags=intro;main;conclusion]";
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert_eq!(
            include.selection,
            tag_selection(&["intro", "main", "conclusion"])
        );
        Ok(())
    }

    #[test]
    fn test_parse_include_with_negated_tag() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[tags=*;!debug]";
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert_eq!(include.selection, tag_selection(&["*", "!debug"]));
        Ok(())
    }

    #[test]
    fn test_parse_include_with_wildcard() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[tags=**]";
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert_eq!(include.selection, tag_selection(&["**"]));
        Ok(())
    }

    #[test]
    fn content_selection_precedence_is_lines_then_tag_then_tags() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let options = Options::default();

        for line in [
            "include::target.adoc[tag=first,tags=second]",
            "include::target.adoc[tags=second,tag=first]",
        ] {
            assert_eq!(
                parse_include(&path, line, &options)?.selection,
                tag_selection(&["first"])
            );
        }
        for line in [
            "include::target.adoc[tag=first,lines=5]",
            "include::target.adoc[lines=5,tag=first]",
        ] {
            assert_eq!(
                parse_include(&path, line, &options)?.selection,
                ContentSelection::Lines(vec![LinesRange::Single(5)])
            );
        }
        Ok(())
    }

    #[test]
    fn duplicate_selection_attributes_are_last_wins() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let options = Options::default();

        assert_eq!(
            parse_include(
                &path,
                "include::target.adoc[tag=first,tag=second]",
                &options,
            )?
            .selection,
            tag_selection(&["second"])
        );
        assert_eq!(
            parse_include(&path, "include::target.adoc[lines=2,lines=5]", &options)?.selection,
            ContentSelection::Lines(vec![LinesRange::Single(5)])
        );
        Ok(())
    }

    #[test]
    fn test_parse_include_with_indent() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[indent=4]";
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert_eq!(include.indent, Some(4));
        Ok(())
    }

    #[test]
    fn include_indent_is_bounded_before_allocation() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from("/tmp");
        let options = Options::default();
        let at_limit = format!("include::target.adoc[indent={MAX_INCLUDE_INDENT}]");
        let include = parse_include(&path, &at_limit, &options)?;
        assert_eq!(include.indent, Some(MAX_INCLUDE_INDENT));

        for over_limit in [MAX_INCLUDE_INDENT + 1, usize::MAX] {
            let directive = format!("include::target.adoc[indent={over_limit}]");
            let Err(error) = parse_include(&path, &directive, &options) else {
                return Err("expected an excessive include indent to be rejected".into());
            };
            let Error::IncludeIndentTooLarge(location, indent, limit) = error else {
                return Err(
                    format!("expected an include-indent limit error, got {error:?}").into(),
                );
            };
            assert_eq!(indent, over_limit);
            assert_eq!(limit, MAX_INCLUDE_INDENT);
            assert_eq!(location.location.start, Position::new(1, 1));
        }
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn optional_missing_include_emits_info_trace() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "acdc-parser-optional-include-info-{}",
            std::process::id()
        ));
        let missing_path = path.join("missing.adoc");
        let options = Options {
            safe_mode: crate::SafeMode::Unsafe,
            ..Options::default()
        };
        let include = parse_include(&path, "include::missing.adoc[opts=optional]", &options)?;
        let source_provider = options
            .include_loader
            .provider()
            .ok_or("system include loader must provide a source provider")?;

        let result = include.process("opts=optional", source_provider)?;

        assert_eq!(result.content, "");
        assert!(include.warnings.borrow().is_empty());
        assert!(logs_contain(
            "optional include dropped because include file not found"
        ));
        assert!(logs_contain(&missing_path.to_string_lossy()));
        Ok(())
    }

    #[test]
    fn test_apply_indent_basic() -> Result<(), Box<dyn std::error::Error>> {
        let (text, shift) = Include::apply_indent(&["def hello", "  puts \"Hello\"", "end"], 4)
            .ok_or("unexpected indent limit")?;
        assert_eq!(text, "    def hello\n      puts \"Hello\"\n    end");
        assert_eq!(shift, 4);
        Ok(())
    }

    #[test]
    fn test_apply_indent_zero() -> Result<(), Box<dyn std::error::Error>> {
        let (text, shift) =
            Include::apply_indent(&["  def hello", "    puts \"Hello\"", "  end"], 0)
                .ok_or("unexpected indent limit")?;
        assert_eq!(text, "def hello\n  puts \"Hello\"\nend");
        assert_eq!(shift, -2);
        Ok(())
    }

    #[test]
    fn test_apply_indent_empty_lines() -> Result<(), Box<dyn std::error::Error>> {
        let (text, shift) =
            Include::apply_indent(&["def hello", "", "  puts \"Hello\"", "   ", "end"], 2)
                .ok_or("unexpected indent limit")?;
        assert_eq!(text, "  def hello\n\n    puts \"Hello\"\n\n  end");
        assert_eq!(shift, 2);
        Ok(())
    }

    #[test]
    fn test_apply_indent_mixed_whitespace() -> Result<(), Box<dyn std::error::Error>> {
        let (text, shift) =
            Include::apply_indent(&["\tdef hello", "\t\tputs \"Hello\"", "\tend"], 2)
                .ok_or("unexpected indent limit")?;
        assert_eq!(text, "  def hello\n  \tputs \"Hello\"\n  end");
        assert_eq!(shift, 1);
        Ok(())
    }
}
