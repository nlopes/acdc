use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

use acdc_core::SafeMode;
use url::Url;

use crate::{
    Options, Preprocessor,
    error::{Error, Positioning, SourceLocation},
    model::{HEADER, Position, Substitute},
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
pub(crate) struct Include {
    file_parent: PathBuf,
    target: Target,
    level_offset: Option<isize>,
    line_range: Vec<LinesRange>,
    tags: Vec<String>,
    indent: Option<usize>,
    encoding: Option<String>,
    opts: Vec<String>,
    options: Options,
    // Location information for error reporting
    line_number: usize,
    current_offset: usize,
    current_file: Option<PathBuf>,
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
/// NOTE: Urls will only be fetched if the attribute `allow-uri-read` is set to `true` (or
/// present).
#[derive(Debug)]
pub(crate) enum Target {
    Path(PathBuf),
    Url(Url),
}

/// Location context for error reporting in include directives
#[derive(Debug, Clone, Copy)]
struct LocationContext<'a> {
    line_number: usize,
    current_offset: usize,
    current_file: Option<&'a Path>,
}

peg::parser! {
    grammar include_parser(
        path: &std::path::Path,
        options: &Options,
        location: LocationContext<'_>
    ) for str {
        pub(crate) rule include() -> Result<Include, Error>
            = "include::" target:target() "[" attrs:attributes()? "]" {
                let target_raw = target.substitute(HEADER, &options.document_attributes);
                let target =
                    if target_raw.starts_with("http://") || target_raw.starts_with("https://") {
                        Target::Url(Url::parse(&target_raw)?)
                    } else {
                        Target::Path(PathBuf::from(target_raw))
                    };

                let mut include = Include {
                    file_parent: path.to_path_buf(),
                    target,
                    level_offset: None,
                    line_range: Vec::new(),
                    tags: Vec::new(),
                    indent: None,
                    encoding: None,
                    opts: Vec::new(),
                    options: options.clone(),
                    line_number: location.line_number,
                    current_offset: location.current_offset,
                    current_file: location.current_file.map(Path::to_path_buf),
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
            = k:$("leveloffset" / "lines" / "tag" / "tags" / "indent" / "encoding" / "opts") {
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
        let (line_number, current_offset, current_file) = location.unwrap_or((1, 0, None));
        Error::InvalidLineRange(
            Box::new(SourceLocation {
                file: current_file.map(Path::to_path_buf),
                positioning: Positioning::Position(Position {
                    line: line_number,
                    column: 1,
                    offset: current_offset,
                }),
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

impl Include {
    fn parse_attributes(&mut self, attributes: Vec<(String, String)>) -> Result<(), Error> {
        for (key, value) in attributes {
            match key.as_ref() {
                "leveloffset" => {
                    self.level_offset = Some(value.parse().map_err(|_| {
                        Error::InvalidLevelOffset(
                            Box::new(SourceLocation {
                                file: self.current_file.clone(),
                                positioning: Positioning::Position(Position {
                                    line: self.line_number,
                                    column: 1,
                                    offset: self.current_offset,
                                }),
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
                "tag" => {
                    self.tags.push(value.clone());
                }
                "tags" => {
                    self.tags.extend(value.split(';').map(str::to_string));
                }
                "indent" => {
                    self.indent = Some(value.parse().map_err(|_| {
                        Error::InvalidIndent(
                            Box::new(SourceLocation {
                                file: self.current_file.clone(),
                                positioning: Positioning::Position(Position {
                                    line: self.line_number,
                                    column: 1,
                                    offset: self.current_offset,
                                }),
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
                            positioning: Positioning::Position(Position {
                                line: self.line_number,
                                column: 1,
                                offset: self.current_offset,
                            }),
                        }),
                        unknown.to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn parse(
        file_parent: &Path,
        line: &str,
        line_number: usize,
        line_start_offset: usize,
        current_file: Option<&Path>,
        options: &Options,
    ) -> Result<Self, Error> {
        let location = LocationContext {
            line_number,
            current_offset: line_start_offset,
            current_file,
        };

        include_parser::include(line, file_parent, options, location).map_err(|e| {
            tracing::error!(?line, error=?e, "failed to parse include directive");
            let location = e.location;
            Error::Parse(
                Box::new(crate::SourceLocation {
                    file: current_file.map(Path::to_path_buf),
                    positioning: crate::Positioning::Position(Position {
                        // Adjust line number to be relative to the document
                        // PEG parser location.line is always 1 for a single line parse
                        line: line_number,
                        column: location.column,
                        // Calculate absolute offset in document:
                        // line_start_offset + column offset (0-indexed)
                        offset: line_start_offset + location.column - 1,
                    }),
                }),
                e.expected.to_string(),
            )
        })?
    }

    pub(crate) fn read_content_from_file(&self, file_path: &Path) -> Result<String, Error> {
        let content =
            crate::preprocessor::read_and_decode_file(file_path, self.encoding.as_deref())?;
        if let Some(ext) = file_path.extension() {
            // If the file is recognized as an AsciiDoc file (i.e., it has one of the
            // following extensions: .asciidoc, .adoc, .ad, .asc, or .txt) additional
            // normalization and processing is performed. First, all trailing whitespace
            // and endlines are removed from each line and replaced with a Unix line feed.
            // This normalization is important to how an AsciiDoc processor works. Next,
            // the AsciiDoc processor runs the preprocessor on the lines, looking for and
            // interpreting the following directives:
            //
            // * includes
            //
            // * preprocessor conditionals (e.g., ifdef)
            //
            // Running the preprocessor on the included content allows includes to be nested, thus
            // provides lot of flexibility in constructing radically different documents with a single
            // primary document and a few command line attributes.
            if ["adoc", "asciidoc", "ad", "asc", "txt"].contains(&ext.to_string_lossy().as_ref()) {
                return super::Preprocessor
                    .process(&content, &self.options)
                    .map_err(|e| {
                        tracing::error!(path=?file_path, error=?e, "failed to process file");
                        e
                    });
            }
        }

        // If we're here, we still need to normalize the content.
        Ok(Preprocessor::normalize(&content))
    }

    pub(crate) fn lines(&self) -> Result<Vec<String>, Error> {
        let mut lines = Vec::new();
        let path = match &self.target {
            Target::Path(path) => self.file_parent.join(path),
            Target::Url(url) => {
                if self.options.safe_mode > SafeMode::Server {
                    tracing::warn!(safe_mode=?self.options.safe_mode, "URL includes are disabled by default. If you want to enable them, must run in `SERVER` mode or less.");
                    return Ok(lines);
                }
                if self
                    .options
                    .document_attributes
                    .get("allow-uri-read")
                    .is_none()
                {
                    tracing::warn!(
                        "URL includes are disabled by default. If you want to enable them, set the 'allow-uri-read' attribute to 'true' in the document attributes or in the command line."
                    );
                    return Ok(lines);
                }
                let mut temp_path = std::env::temp_dir();
                if let Some(file_name) = url.path_segments().and_then(std::iter::Iterator::last) {
                    temp_path.push(file_name);
                } else {
                    tracing::error!(url=?url, "failed to extract file name from URL");
                    return Ok(lines);
                }
                {
                    let mut response = ureq::get(url.as_str())
                        .call()
                        .map_err(|e| Error::HttpRequest(e.to_string()))?;
                    // Create and write to the file
                    let mut file = File::create(&temp_path)?;
                    io::copy(&mut response.body_mut().as_reader(), &mut file)?;
                }
                tracing::debug!(?temp_path, url=?url, "downloaded file from URL");
                temp_path
            }
        };
        // If the path doesn't exist, we still need to return an empty list of
        // lines because we never want to fail parsing the doc because of an
        // include directive.
        if !path.exists() {
            // If the include is not optional, we log a warning though!
            if !self.opts.contains(&"optional".to_string()) {
                tracing::warn!(
                    path=?path,
                    "file is missing - include directive won't be processed"
                );
            }
            return Ok(lines);
        }
        let content = self.read_content_from_file(&path)?;

        if let Some(level_offset) = self.level_offset {
            tracing::warn!(level_offset, "level offset is not supported yet");
        }
        if let Some(indent) = self.indent {
            tracing::warn!(indent, "indent is not supported yet");
        }
        // TODO(nlopes): this is so unoptimized, it isn't even funny but I'm
        // trying to just get to a place of compatibility, then I can
        // optimize.
        let content_lines = content.lines().map(str::to_string).collect::<Vec<_>>();
        if !self.tags.is_empty() {
            tracing::warn!(tags = ?self.tags, "tags are not supported yet");
        }

        if self.line_range.is_empty() {
            lines.extend(content_lines);
        } else {
            self.extend_lines_with_ranges(&content_lines, &mut lines);
        }
        Ok(lines)
    }

    fn validate_line_number(num: usize) -> Option<usize> {
        if num < 1 {
            tracing::warn!(?num, "invalid line number in include directive");
            None
        } else {
            Some(num - 1)
        }
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

    pub(crate) fn extend_lines_with_ranges(
        &self,
        content_lines: &[String],
        lines: &mut Vec<String>,
    ) {
        let content_lines_count = content_lines.len();
        for line in &self.line_range {
            match line {
                LinesRange::Single(line_number) => {
                    if let Some(idx) = Self::validate_line_number(*line_number)
                        && idx < content_lines_count
                    {
                        lines.push(content_lines[idx].clone());
                    }
                }
                LinesRange::Range(start, end) => {
                    let Some(start_idx) = Self::validate_line_number(*start) else {
                        continue;
                    };
                    let Some(end_idx) = Self::resolve_end_line(*end, content_lines_count) else {
                        continue;
                    };

                    if start_idx < content_lines_count && end_idx < content_lines_count {
                        lines.extend_from_slice(&content_lines[start_idx..=end_idx]);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_simple_include() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[]";
        let options = Options::default();
        let include = Include::parse(&path, line, 1, 0, None, &options)?;

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
        let include = Include::parse(&path, line, 1, 0, None, &options)?;

        assert_eq!(include.level_offset, Some(1));
        assert_eq!(include.tags, vec!["example"]);
        assert!(!include.line_range.is_empty());
        Ok(())
    }

    #[test]
    fn test_parse_include_with_url() -> Result<(), Error> {
        let path = PathBuf::from("/tmp");
        let line = "include::https://example.com/doc.adoc[]";
        let options = Options::default();
        let include = Include::parse(&path, line, 1, 0, None, &options)?;

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
        let include = Include::parse(&path, line, 1, 0, None, &options)?;

        assert_eq!(include.tags, vec!["example code"]);
        assert_eq!(include.encoding, Some("utf-8".to_string()));
        Ok(())
    }
}
