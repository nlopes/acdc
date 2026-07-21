use std::{
    cell::RefCell,
    path::{Path, PathBuf},
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
    SourceOrigin,
    tag::{DELIMITERS, Filter as TagFilter, Name as TagName, apply_tag_filters},
};

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
    /// Immutable snapshot of whether the caller supplied `allow-uri-read`.
    caller_allows_uri_read: bool,
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
    caller_allows_uri_read: bool,
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
                    caller_allows_uri_read: inputs.caller_allows_uri_read,
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

impl IncludeResult {
    fn empty() -> Self {
        Self {
            lines: Vec::new(),
            source_lines: Vec::new(),
            column_shift: 0,
            effective_leveloffset: None,
            nested_leveloffset_ranges: Vec::new(),
            file: None,
            target: String::new(),
            nested_source_ranges: Vec::new(),
        }
    }

    fn secure_fallback(target: &str) -> Self {
        Self {
            lines: vec![format!("link:{target}[role=include]")],
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
                    self.indent = Some(value.parse().map_err(|_| {
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
                    })?);
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
        caller_allows_uri_read: bool,
        warnings: &Rc<RefCell<Vec<crate::Warning>>>,
    ) -> Result<Self, Error> {
        let inputs = IncludeParserInputs {
            source_origin,
            options,
            caller_allows_uri_read,
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

    /// Fetch a URL target into memory without changing its source origin.
    /// Returns Ok(None) if URL includes are disabled (safe mode, missing attribute).
    /// Returns Err for actual network or response-read failures.
    #[allow(clippy::unnecessary_wraps)] // Err is used when "network" feature is enabled
    fn fetch_url_target(&self, url: &str) -> Result<Option<Vec<u8>>, Error> {
        if self.options.safe_mode > SafeMode::Server {
            self.warn_unlocated(
                "URL includes are disabled by default. Run in `SERVER` mode or less to enable.",
            );
            return Ok(None);
        }
        if !self.caller_allows_uri_read {
            self.warn_unlocated(
                "URL includes are disabled by default. Set the 'allow-uri-read' attribute to 'true' to enable.",
            );
            return Ok(None);
        }

        #[cfg(not(feature = "network"))]
        {
            self.warn_unlocated(format!(
                "network support is disabled, cannot fetch remote includes: {url}",
            ));
            Ok(None)
        }

        #[cfg(feature = "network")]
        {
            let mut response = ureq::get(url)
                .call()
                .map_err(|e| Error::HttpRequest(e.to_string()))?;
            let mut bytes = Vec::new();
            response.body_mut().as_reader().read_to_end(&mut bytes)?;

            tracing::debug!(%url, "downloaded content from URL");
            Ok(Some(bytes))
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

        super::Preprocessor {
            warnings: Rc::clone(&self.warnings),
            caller_allows_uri_read: self.caller_allows_uri_read,
        }
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

    /// Read and process content from a local file.
    pub(crate) fn read_content_from_file(
        &self,
        file_path: &Path,
    ) -> Result<IncludedContent, Error> {
        let content = super::read_and_decode_file(file_path, self.encoding.as_deref())?;
        let source_origin = SourceOrigin::File(file_path.to_path_buf());
        self.process_content(
            &content,
            &source_origin,
            Self::has_asciidoc_extension(file_path),
        )
    }

    /// Fetch and process content from a URI without converting it to a local origin.
    fn read_content_from_url(&self, url: &str) -> Result<Option<IncludedContent>, Error> {
        let Some(bytes) = self.fetch_url_target(url)? else {
            return Ok(None);
        };
        let content = super::decode_bytes(&bytes, self.encoding.as_deref(), url)?;
        let parsed_url = Url::parse(url)?;
        let source_origin = SourceOrigin::Uri(url.to_string());
        self.process_content(
            &content,
            &source_origin,
            Self::has_asciidoc_extension(Path::new(parsed_url.path())),
        )
        .map(Some)
    }

    pub(crate) fn lines(&self) -> Result<IncludeResult, Error> {
        if self.options.safe_mode == SafeMode::Secure {
            return Ok(IncludeResult::secure_fallback(self.target_as_written()));
        }

        let (content, nested_leveloffset_ranges, nested_source_ranges, resolved_source) =
            match &self.target {
                Target::Path(target) => {
                    let SourceOrigin::File(current_file) = &self.source_origin else {
                        tracing::error!(?target, "local include target has a URI source origin");
                        return Ok(IncludeResult::empty());
                    };
                    let Some(parent) = current_file.parent() else {
                        tracing::error!(?current_file, "source file has no parent directory");
                        return Ok(IncludeResult::empty());
                    };
                    let path = parent.join(target);
                    if !path.exists() {
                        if !self.opts.contains(&"optional".to_string()) {
                            self.warn_located(format!(
                                "file is missing — include directive won't be processed: {}",
                                path.display(),
                            ));
                        }
                        return Ok(IncludeResult::empty());
                    }
                    let (content, leveloffset_ranges, source_ranges) =
                        self.read_content_from_file(&path)?;
                    (content, leveloffset_ranges, source_ranges, path)
                }
                Target::Url(url) => {
                    let Some((content, leveloffset_ranges, source_ranges)) =
                        self.read_content_from_url(url)?
                    else {
                        return Ok(IncludeResult::empty());
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

    /// Push a warning with no source location — used for configuration-
    /// level conditions (disabled URL includes, network feature off)
    /// that aren't tied to a specific include line beyond "this parse".
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
        let source_origin = SourceOrigin::File(path.join("source.adoc"));
        Include::parse(
            &source_origin,
            line,
            LocationContext::new(1, 0, None),
            options,
            false,
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
