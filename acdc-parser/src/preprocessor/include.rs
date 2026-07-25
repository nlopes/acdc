use std::{
    cell::RefCell,
    path::{Component, Path, PathBuf},
    rc::Rc,
    str::FromStr,
};

#[cfg(feature = "network")]
use std::io::Read;

use url::Url;

use crate::{
    Options, Preprocessor, SafeMode,
    error::{Error, SourceLocation},
    model::{HEADER, LeveloffsetRange, Position, SourceRange, substitute},
};

use super::{
    IncludeContext, SourceOrigin,
    tag::{DELIMITERS, Filter as TagFilter, Name as TagName, apply_tag_filters},
};

#[cfg(feature = "network")]
const MAX_REMOTE_INCLUDE_BYTES: usize = 10 * 1024 * 1024;
/// Maximum number of spaces one include directive may prepend to each non-empty
/// line. This bounds the allocation controlled by a single `indent` value; a
/// parse-wide expansion budget remains a separate concern.
const MAX_INCLUDE_INDENT: usize = 4 * 1024;

#[cfg(feature = "network")]
fn read_remote_include(reader: impl Read) -> Result<Vec<u8>, Error> {
    let read_limit = u64::try_from(MAX_REMOTE_INCLUDE_BYTES + 1)?;
    let mut bytes = Vec::new();
    reader.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_REMOTE_INCLUDE_BYTES {
        return Err(Error::HttpRequest(
            "remote include response exceeds the 10 MiB limit".to_string(),
        ));
    }
    Ok(bytes)
}

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
    line_range: Vec<LinesRange>,
    tags: Vec<TagName>,
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
    warnings: Rc<RefCell<Vec<crate::Warning>>>,
}

/// A line range that an include may specify.
///
/// If the range contains `..` then it is a range of lines, if not, it is parsed as a
/// single line.
///
/// There can be multiple of these in an include definition.
#[derive(Debug)]
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
}

impl Target {
    fn parse(target: &str, source_origin: &SourceOrigin) -> Result<Self, Error> {
        if target.starts_with("http://") || target.starts_with("https://") {
            Url::parse(target)?;
            return Ok(Self::Url(target.to_string()));
        }

        if let SourceOrigin::Uri(containing_uri) = source_origin {
            let uri = format!("{}/{target}", uri_directory(containing_uri));
            Url::parse(&uri)?;
            return Ok(Self::Url(uri));
        }

        Ok(Self::Path(PathBuf::from(target)))
    }
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
    warnings: &'b Rc<RefCell<Vec<crate::Warning>>>,
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
                    line_range: Vec::new(),
                    tags: Vec::new(),
                    indent: None,
                    encoding: None,
                    opts: Vec::new(),
                    options: inputs.options.clone(),
                    context: inputs.context,
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
            = t:$((!['[' | ' ' | '\t'] [_])+) {
                t.to_string()
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
                location: crate::Location::point(Position::from_line_col(line_number, 1)),
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

/// Origin of a single emitted include line: its 1-indexed line number within the
/// included file and the byte offset of the line's first byte in that file (both
/// relative to the file's normalized content). Lets the caller record one
/// [`SourceRange`] per run of consecutive source lines, so partial (`lines=` /
/// `tags=`) includes map back to their true origin lines rather than to line 1.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IncludedLineOrigin {
    pub(crate) line: usize,
    pub(crate) offset: usize,
}

/// Result of processing an include directive.
///
/// Contains the included lines and any leveloffset that should apply to them.
#[derive(Debug)]
pub(crate) struct IncludeResult {
    pub(crate) lines: Vec<String>,
    /// Directive line to which synthesized fallback content belongs. Unlike
    /// content read from another file, this line maps to the including source.
    pub(crate) synthetic_source_line: Option<usize>,
    /// Origin (1-indexed line + byte offset within the included file) of each line
    /// in `lines`, parallel to it. A whole-file include is lines `1..N`; partial
    /// includes carry the selected lines' true origins.
    pub(crate) source_lines: Vec<IncludedLineOrigin>,
    /// Per-line column shift applied by `indent=N` (`N − common_indent`), uniform
    /// across the include; `0` when no `indent` was given (content copied verbatim).
    pub(crate) column_shift: isize,
    /// The effective leveloffset value to apply to this included content.
    /// This is the sum of the current document's leveloffset and the include's leveloffset.
    pub(crate) effective_leveloffset: Option<isize>,
    /// Leveloffset ranges from nested includes within this included file.
    /// These need to be merged into the parent's ranges with adjusted byte offsets.
    pub(crate) nested_leveloffset_ranges: Vec<LeveloffsetRange>,
    /// The resolved file path of the included content.
    pub(crate) file: Option<PathBuf>,
    /// The include target exactly as written in the directive (after attribute
    /// substitution), e.g. `markup.adoc` or `chapters/intro.adoc`. Used as the
    /// outermost element of the ASG `file` include chain. Empty when no target
    /// resolved (missing/optional include).
    pub(crate) target: String,
    /// Source ranges from nested includes within this included file.
    /// These need to be merged into the parent's ranges with adjusted byte offsets.
    pub(crate) nested_source_ranges: Vec<SourceRange>,
}

type IncludedContent = (String, Vec<LeveloffsetRange>, Vec<SourceRange>);

enum UrlReadError {
    #[cfg(not(feature = "network"))]
    NetworkDisabled,
    #[cfg(feature = "network")]
    Retrieval(String),
    Other(Error),
}

enum UrlIncludeOutcome {
    Content(IncludedContent),
    Fallback(IncludeResult),
}

impl From<Error> for UrlReadError {
    fn from(error: Error) -> Self {
        Self::Other(error)
    }
}

impl IncludeResult {
    fn empty() -> Self {
        Self {
            lines: Vec::new(),
            synthetic_source_line: None,
            source_lines: Vec::new(),
            column_shift: 0,
            effective_leveloffset: None,
            nested_leveloffset_ranges: Vec::new(),
            file: None,
            target: String::new(),
            nested_source_ranges: Vec::new(),
        }
    }

    fn link_fallback(target: &str, source_line: usize) -> Self {
        Self {
            lines: vec![format!("link:{target}[role=include]")],
            synthetic_source_line: Some(source_line),
            source_lines: Vec::new(),
            column_shift: 0,
            effective_leveloffset: None,
            nested_leveloffset_ranges: Vec::new(),
            file: None,
            target: String::new(),
            nested_source_ranges: Vec::new(),
        }
    }

    fn unresolved_directive(directive: String, source_line: usize) -> Self {
        Self {
            lines: vec![directive],
            synthetic_source_line: Some(source_line),
            source_lines: Vec::new(),
            column_shift: 0,
            effective_leveloffset: None,
            nested_leveloffset_ranges: Vec::new(),
            file: None,
            target: String::new(),
            nested_source_ranges: Vec::new(),
        }
    }
}

impl<'a> Include<'a> {
    fn parse_attributes(&mut self, attributes: Vec<(String, String)>) -> Result<(), Error> {
        for (key, value) in attributes {
            match key.as_ref() {
                "leveloffset" => {
                    self.level_offset = Some(value.parse().map_err(|_| {
                        Error::InvalidLevelOffset(
                            Box::new(SourceLocation {
                                file: self.current_file.clone(),
                                location: crate::Location::point(Position::from_line_col(
                                    self.line_number,
                                    1,
                                )),
                            }),
                            value.clone(),
                        )
                    })?);
                }
                "lines" => {
                    self.line_range.extend(LinesRange::parse(
                        &value,
                        self.line_number,
                        self.current_offset,
                        self.current_file.as_deref(),
                    )?);
                }
                "tag" => self.tags.push(TagName::from(value)),
                "tags" => {
                    self.tags.extend(value.split(DELIMITERS).map(TagName::from));
                }
                "indent" => {
                    let indent = value.parse().map_err(|_| {
                        Error::InvalidIndent(
                            Box::new(SourceLocation {
                                file: self.current_file.clone(),
                                location: crate::Location::point(Position::from_line_col(
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
                                location: crate::Location::point(Position::from_line_col(
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
                            location: crate::Location::point(Position::from_line_col(
                                self.line_number,
                                1,
                            )),
                        }),
                        unknown.to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn parse(
        source_origin: &SourceOrigin,
        line: &str,
        location: LocationContext<'_>,
        options: &Options<'a>,
        include_context: IncludeContext,
        warnings: &Rc<RefCell<Vec<crate::Warning>>>,
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
                    location: crate::Location::point(Position::from_line_col(
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

    fn unresolved_directive_for_target(
        &self,
        target_as_written: &str,
        attribute_list_as_written: &str,
    ) -> IncludeResult {
        let source = match &self.source_origin {
            SourceOrigin::File { path, base_dir } => {
                let absolute_path =
                    super::absolute_normalized(path).unwrap_or_else(|_| path.clone());
                absolute_path
                    .strip_prefix(base_dir)
                    .unwrap_or(&absolute_path)
                    .display()
                    .to_string()
            }
            SourceOrigin::Uri(uri) => uri.clone(),
        };
        IncludeResult::unresolved_directive(
            format!(
                "Unresolved directive in {source} - include::{target_as_written}[{attribute_list_as_written}]"
            ),
            self.line_number,
        )
    }

    /// Build Asciidoctor's visible recovery line for a failed local include read.
    fn unresolved_directive(&self, attribute_list_as_written: &str) -> IncludeResult {
        self.unresolved_directive_for_target(self.target_as_written(), attribute_list_as_written)
    }

    /// Escape the URI macro in generated parser input so the unresolved directive
    /// remains converter-visible plain text. Inline parsing removes the backslash.
    fn unresolved_uri_directive(&self, attribute_list_as_written: &str) -> IncludeResult {
        self.unresolved_directive_for_target(
            &format!(r"\{}", self.target_as_written()),
            attribute_list_as_written,
        )
    }

    /// Fetch a URL target into memory without changing its source origin.
    /// Request-opening failures remain distinct from response-read failures so
    /// callers can recover without also swallowing size or body-read errors.
    fn fetch_url_target(url: &str) -> Result<Vec<u8>, UrlReadError> {
        #[cfg(not(feature = "network"))]
        {
            let _ = url;
            Err(UrlReadError::NetworkDisabled)
        }

        #[cfg(feature = "network")]
        {
            let mut response = ureq::get(url)
                .call()
                .map_err(|error| UrlReadError::Retrieval(error.to_string()))?;
            // Apply the cap after transport decoding so compressed responses cannot
            // expand beyond the parser's per-include memory boundary.
            let bytes = match read_remote_include(response.body_mut().as_reader()) {
                Ok(bytes) => bytes,
                // Let ureq validate HTTP framing and transport decoding. Any body I/O
                // failure follows the same parser recovery as a request-opening error;
                // partial bytes are deliberately discarded.
                Err(Error::Io(error)) => {
                    return Err(UrlReadError::Retrieval(error.to_string()));
                }
                Err(error) => return Err(UrlReadError::Other(error)),
            };

            tracing::debug!(%url, "downloaded content from URL");
            Ok(bytes)
        }
    }

    /// Apply tag and line range filters to content lines, returning the surviving
    /// lines together with each one's 0-indexed position in `content_lines`. The
    /// caller maps those indices to origin line/offset so partial includes locate
    /// their content correctly. Both vectors are parallel and equal length.
    fn apply_content_filters(&self, content_lines: &[String]) -> (Vec<String>, Vec<usize>) {
        let mut lines = Vec::new();
        let mut indices = Vec::new();

        if !self.tags.is_empty() {
            let filters: Vec<TagFilter> = self
                .tags
                .iter()
                .map(|t| TagFilter::parse(t.as_str()))
                .collect();
            let selected_indices = apply_tag_filters(content_lines, &filters);

            if self.line_range.is_empty() {
                for idx in selected_indices {
                    if let Some(line) = content_lines.get(idx) {
                        lines.push(line.clone());
                        indices.push(idx);
                    }
                }
            } else {
                let line_range_indices = self.collect_line_range_indices(content_lines.len());
                for idx in selected_indices {
                    if line_range_indices.contains(&idx)
                        && let Some(line) = content_lines.get(idx)
                    {
                        lines.push(line.clone());
                        indices.push(idx);
                    }
                }
            }
        } else if self.line_range.is_empty() {
            lines.extend(content_lines.iter().cloned());
            indices.extend(0..content_lines.len());
        } else {
            self.extend_lines_with_ranges(content_lines, &mut lines, &mut indices);
        }

        (lines, indices)
    }

    /// Re-indent `lines`: strip the block's common leading whitespace, then prepend
    /// `indent` spaces. Returns the rewritten lines and the uniform per-line column
    /// shift (`indent − common_indent`), which the remap subtracts to recover origin
    /// columns. The shift is in characters; it equals the byte shift for the usual
    /// ASCII (space/tab) leading whitespace.
    fn apply_indent(lines: &[String], indent: usize) -> (Vec<String>, isize) {
        let min_indent = lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);

        let prefix = " ".repeat(indent);
        let indented = lines
            .iter()
            .map(|line| {
                if line.trim().is_empty() {
                    String::new()
                } else {
                    let stripped = if min_indent > 0 {
                        &line[min_indent..]
                    } else {
                        line.as_str()
                    };
                    format!("{prefix}{stripped}")
                }
            })
            .collect();
        let column_shift =
            isize::try_from(indent).unwrap_or(0) - isize::try_from(min_indent).unwrap_or(0);
        (indented, column_shift)
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
            let target = super::absolute_normalized(target)?;
            if target.starts_with(base_dir) {
                return Ok(target);
            }
            self.warn_unlocated("include file is outside of jail; recovering automatically");
            return Ok(Self::rebase_absolute_target(base_dir, &target));
        }

        let mut resolved = super::absolute_normalized(current_parent)?;
        if !resolved.starts_with(base_dir) {
            self.warn_unlocated("include file is outside of jail; recovering automatically");
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
            self.warn_unlocated(
                "include file has illegal reference to ancestor of jail; recovering automatically",
            );
        }
        Ok(resolved)
    }

    /// Process included content while retaining the origin used for nested includes.
    fn process_content(
        &self,
        content: &str,
        source_origin: &SourceOrigin,
        is_asciidoc: bool,
    ) -> Result<IncludedContent, Error> {
        if !is_asciidoc {
            return Ok((
                Preprocessor::normalize(content).into_owned(),
                Vec::new(),
                Vec::new(),
            ));
        }

        super::Preprocessor::nested(&self.warnings, self.context)
            .process_inner(content, Some(source_origin), &self.options)
        .map(|result| {
            (
                result.text.into_owned(),
                result.leveloffset_ranges,
                result.source_ranges,
            )
        })
        .map_err(|error| {
            tracing::error!(origin=?source_origin, ?error, "failed to process included content");
            error
        })
    }

    /// Fetch and process content from a URI without converting it to a local origin.
    fn read_content_from_url(&self, url: &str) -> Result<IncludedContent, UrlReadError> {
        let bytes = Self::fetch_url_target(url)?;
        let content = super::decode_bytes(&bytes, self.encoding.as_deref(), url)?;
        let parsed_url = Url::parse(url).map_err(Error::from)?;
        let source_origin = SourceOrigin::Uri(url.to_string());
        self.process_content(
            &content,
            &source_origin,
            Self::has_asciidoc_extension(Path::new(parsed_url.path())),
        )
        .map_err(UrlReadError::from)
    }

    fn read_url_content_or_fallback(
        &self,
        url: &str,
        attribute_list_as_written: &str,
    ) -> Result<UrlIncludeOutcome, Error> {
        if !self.context.allows_uri_read {
            return Ok(UrlIncludeOutcome::Fallback(IncludeResult::link_fallback(
                self.target_as_written(),
                self.line_number,
            )));
        }

        match self.read_content_from_url(url) {
            Ok(content) => Ok(UrlIncludeOutcome::Content(content)),
            #[cfg(not(feature = "network"))]
            Err(UrlReadError::NetworkDisabled) => {
                self.warn_located(format!(
                    "network support is disabled, cannot fetch remote includes: {url}",
                ));
                Ok(UrlIncludeOutcome::Fallback(
                    self.unresolved_uri_directive(attribute_list_as_written),
                ))
            }
            #[cfg(feature = "network")]
            Err(UrlReadError::Retrieval(detail)) => {
                tracing::debug!(%url, %detail, "failed to retrieve remote include");
                self.warn_located(format!("include uri not readable: {url}"));
                Ok(UrlIncludeOutcome::Fallback(
                    self.unresolved_uri_directive(attribute_list_as_written),
                ))
            }
            Err(UrlReadError::Other(error)) => Err(error),
        }
    }

    /// Read one resolved local file. `None` is the recoverable non-optional
    /// unreadable case; errors raised while preprocessing its content remain
    /// distinct and propagate with their own source context.
    fn read_existing_local_content(
        &self,
        path: &Path,
        base_dir: &Path,
        optional: bool,
    ) -> Result<Option<IncludedContent>, Error> {
        let decoded = match super::read_and_decode_file(path, self.encoding.as_deref()) {
            Ok(content) => content,
            Err(Error::Io(_)) if !optional => {
                self.warn_located(format!("include file not readable: {}", path.display()));
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let source_origin = SourceOrigin::File {
            path: path.to_path_buf(),
            base_dir: base_dir.to_path_buf(),
        };
        self.process_content(&decoded, &source_origin, Self::has_asciidoc_extension(path))
            .map(Some)
    }

    pub(crate) fn lines(&self, attribute_list_as_written: &str) -> Result<IncludeResult, Error> {
        if self.options.safe_mode == SafeMode::Secure {
            return Ok(IncludeResult::link_fallback(
                self.target_as_written(),
                self.line_number,
            ));
        }

        let (content, nested_leveloffset_ranges, nested_source_ranges, resolved_source) =
            match &self.target {
                Target::Path(target) => {
                    let SourceOrigin::File {
                        path: current_file,
                        base_dir,
                    } = &self.source_origin
                    else {
                        tracing::error!(?target, "local include target has a URI source origin");
                        return Ok(IncludeResult::empty());
                    };
                    let Some(parent) = current_file.parent() else {
                        tracing::error!(?current_file, "source file has no parent directory");
                        return Ok(IncludeResult::empty());
                    };
                    let path = self.resolve_file_target(parent, base_dir, target)?;
                    let optional = self.opts.iter().any(|option| option == "optional");
                    if !path.is_file() {
                        if optional {
                            tracing::info!(
                                source_file = ?self.current_file,
                                line = self.line_number,
                                include_path = %path.display(),
                                "optional include dropped because include file not found",
                            );
                        } else {
                            self.warn_located(format!(
                                "include file not found: {}",
                                path.display()
                            ));
                            return Ok(self.unresolved_directive(attribute_list_as_written));
                        }
                        return Ok(IncludeResult::empty());
                    }
                    let Some((content, leveloffset_ranges, source_ranges)) =
                        self.read_existing_local_content(&path, base_dir, optional)?
                    else {
                        return Ok(self.unresolved_directive(attribute_list_as_written));
                    };
                    (content, leveloffset_ranges, source_ranges, path)
                }
                Target::Url(url) => {
                    let (content, leveloffset_ranges, source_ranges) =
                        match self.read_url_content_or_fallback(url, attribute_list_as_written)? {
                            UrlIncludeOutcome::Content(content) => content,
                            UrlIncludeOutcome::Fallback(result) => return Ok(result),
                        };
                    (
                        content,
                        leveloffset_ranges,
                        source_ranges,
                        PathBuf::from(url),
                    )
                }
            };
        let effective_leveloffset = self.calculate_effective_leveloffset();

        let content_lines = content.lines().map(str::to_string).collect::<Vec<_>>();
        let (lines, selected_indices) = self.apply_content_filters(&content_lines);
        let (lines, column_shift) = if let Some(indent) = self.indent {
            Self::apply_indent(&lines, indent)
        } else {
            (lines, 0)
        };

        // Map each surviving line back to its origin line/offset in the included
        // file, so the caller can split partial includes into correctly-located runs.
        let line_starts = Self::line_start_offsets(&content_lines);
        let source_lines = selected_indices
            .iter()
            .map(|&idx| IncludedLineOrigin {
                line: idx + 1,
                offset: line_starts.get(idx).copied().unwrap_or(0),
            })
            .collect();

        Ok(IncludeResult {
            lines,
            synthetic_source_line: None,
            source_lines,
            column_shift,
            effective_leveloffset,
            nested_leveloffset_ranges,
            file: Some(resolved_source),
            target: self.target_as_written().to_string(),
            nested_source_ranges,
        })
    }

    /// Calculate the effective leveloffset for this include.
    /// This is the sum of the current document's leveloffset and the include's leveloffset.
    fn calculate_effective_leveloffset(&self) -> Option<isize> {
        self.level_offset.map(|level_offset| {
            let current_offset = self
                .options
                .document_attributes
                .get_string("leveloffset")
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
            location: crate::Location::point(crate::Position::from_line_col(self.line_number, 1)),
        };
        let warning = crate::Warning::new(
            crate::WarningKind::Other(message.into()),
            Some(source_location),
        );
        tracing::warn!("{warning}");
        self.warnings.borrow_mut().push(warning);
    }

    /// Push a warning with no source location. The remaining callers are
    /// Safe/Server jail-recovery conditions whose location contract is tracked
    /// separately from directive-specific read failures.
    fn warn_unlocated(&self, message: impl Into<std::borrow::Cow<'static, str>>) {
        let warning = crate::Warning::new(crate::WarningKind::Other(message.into()), None);
        tracing::warn!("{warning}");
        self.warnings.borrow_mut().push(warning);
    }

    fn resolve_end_line(end: isize, max_size: usize) -> Option<usize> {
        match end {
            -1 => Some(max_size),
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
    fn collect_line_range_indices(
        &self,
        content_lines_count: usize,
    ) -> std::collections::HashSet<usize> {
        let mut indices = std::collections::HashSet::new();
        for line in &self.line_range {
            match line {
                LinesRange::Single(line_number) => {
                    if let Some(idx) = self.validate_line_number(*line_number) {
                        if idx < content_lines_count {
                            indices.insert(idx);
                        }
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

    pub(crate) fn extend_lines_with_ranges(
        &self,
        content_lines: &[String],
        lines: &mut Vec<String>,
        indices: &mut Vec<usize>,
    ) {
        let content_lines_count = content_lines.len();
        for line in &self.line_range {
            match line {
                LinesRange::Single(line_number) => {
                    if let Some(idx) = self.validate_line_number(*line_number)
                        && idx < content_lines_count
                        && let Some(line) = content_lines.get(idx)
                    {
                        lines.push(line.clone());
                        indices.push(idx);
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
                        for idx in start_idx..=end_idx {
                            if let Some(line) = content_lines.get(idx) {
                                lines.push(line.clone());
                                indices.push(idx);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Byte offset of each line's first byte within the file's normalized content
    /// (lines joined with a single `\n`), so a surviving line can carry its true
    /// origin-file byte offset.
    fn line_start_offsets(content_lines: &[String]) -> Vec<usize> {
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

    fn parse_include<'a>(
        path: &Path,
        line: &str,
        options: &Options<'a>,
    ) -> Result<Include<'a>, Error> {
        let source_origin = SourceOrigin::File {
            path: path.join("source.adoc"),
            base_dir: path.to_path_buf(),
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
        assert_eq!(include.tags, vec![TagName::from("example")]);
        assert!(!include.line_range.is_empty());
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

        assert_eq!(include.tags, vec![TagName::from("example code")]);
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
            include.tags,
            vec![
                TagName::from("intro"),
                TagName::from("main"),
                TagName::from("conclusion")
            ]
        );
        Ok(())
    }

    #[test]
    fn test_parse_include_with_negated_tag() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[tags=*;!debug]";
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert_eq!(
            include.tags,
            vec![TagName::from("*"), TagName::from("!debug")]
        );
        Ok(())
    }

    #[test]
    fn test_parse_include_with_wildcard() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[tags=**]";
        let options = Options::default();
        let include = parse_include(&path, line, &options)?;

        assert_eq!(include.tags, vec![TagName::from("**")]);
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
    fn optional_missing_include_emits_info_trace() -> Result<(), Error> {
        let path = std::env::temp_dir().join(format!(
            "acdc-parser-optional-include-info-{}",
            std::process::id()
        ));
        let missing_path = path.join("missing.adoc");
        let options = Options::default();
        let include = parse_include(&path, "include::missing.adoc[opts=optional]", &options)?;

        let result = include.lines("opts=optional")?;

        assert!(result.lines.is_empty());
        assert!(include.warnings.borrow().is_empty());
        assert!(logs_contain(
            "optional include dropped because include file not found"
        ));
        assert!(logs_contain(&missing_path.to_string_lossy()));
        Ok(())
    }

    #[cfg(feature = "network")]
    #[test]
    fn remote_include_response_is_limited_to_ten_mib() -> Result<(), Box<dyn std::error::Error>> {
        let accepted = read_remote_include(
            std::io::repeat(b'a').take(u64::try_from(MAX_REMOTE_INCLUDE_BYTES)?),
        )?;
        assert_eq!(accepted.len(), MAX_REMOTE_INCLUDE_BYTES);
        drop(accepted);

        let Err(error) = read_remote_include(std::io::repeat(b'a')) else {
            return Err("expected an oversized remote include to be rejected".into());
        };
        let Error::HttpRequest(message) = error else {
            return Err(format!("expected an HTTP request error, got {error:?}").into());
        };
        assert_eq!(message, "remote include response exceeds the 10 MiB limit");
        Ok(())
    }

    #[test]
    fn test_apply_indent_basic() {
        // min indent is 0 (def hello, end), so indent=4 adds 4 spaces to all
        let lines = vec![
            "def hello".to_string(),
            "  puts \"Hello\"".to_string(),
            "end".to_string(),
        ];
        let (result, column_shift) = Include::apply_indent(&lines, 4);
        assert_eq!(
            result,
            vec!["    def hello", "      puts \"Hello\"", "    end",]
        );
        assert_eq!(column_shift, 4); // 4 added − 0 common
    }

    #[test]
    fn test_apply_indent_zero() {
        // min indent is 2, so indent=0 strips 2 spaces from all lines
        let lines = vec![
            "  def hello".to_string(),
            "    puts \"Hello\"".to_string(),
            "  end".to_string(),
        ];
        let (result, column_shift) = Include::apply_indent(&lines, 0);
        assert_eq!(result, vec!["def hello", "  puts \"Hello\"", "end",]);
        assert_eq!(column_shift, -2); // 0 added − 2 common stripped
    }

    #[test]
    fn test_apply_indent_empty_lines() {
        // min indent is 0 (def hello, end), empty/whitespace-only lines become empty
        let lines = vec![
            "def hello".to_string(),
            String::new(),
            "  puts \"Hello\"".to_string(),
            "   ".to_string(),
            "end".to_string(),
        ];
        let (result, column_shift) = Include::apply_indent(&lines, 2);
        assert_eq!(
            result,
            vec!["  def hello", "", "    puts \"Hello\"", "", "  end",]
        );
        assert_eq!(column_shift, 2); // 2 added − 0 common
    }

    #[test]
    fn test_apply_indent_mixed_whitespace() {
        // min indent is 1 (tab counts as 1 char), strips 1 char from all
        let lines = vec![
            "\tdef hello".to_string(),
            "\t\tputs \"Hello\"".to_string(),
            "\tend".to_string(),
        ];
        let (result, column_shift) = Include::apply_indent(&lines, 2);
        assert_eq!(result, vec!["  def hello", "  \tputs \"Hello\"", "  end",]);
        assert_eq!(column_shift, 1); // 2 added − 1 common (tab counts as one char)
    }
}
