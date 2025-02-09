use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use url::Url;

use crate::{
    error::Error,
    model::{Substitute, HEADER},
    DocumentAttributes,
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
    lines: Vec<LinesRange>,
    tags: Vec<String>,
    indent: Option<usize>,
    encoding: Option<String>,
    opts: Vec<String>,
    attributes: DocumentAttributes,
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

peg::parser! {
    grammar include_parser(path: &std::path::Path, attributes: &DocumentAttributes) for str {
        pub(crate) rule include() -> Result<Include, Error>
            = "include::" target:target() "[" attrs:attributes()? "]" {
                let target_raw = target.substitute(HEADER, attributes);
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
                    lines: Vec::new(),
                    tags: Vec::new(),
                    indent: None,
                    encoding: None,
                    opts: Vec::new(),
                    attributes: attributes.clone(),
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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains("..") {
            let mut parts = s.split("..");
            let start = parts.next().expect("no start").parse()?;
            let end = parts.next().expect("no end").parse()?;
            Ok(LinesRange::Range(start, end))
        } else {
            Ok(LinesRange::Single(s.parse().map_err(|e| {
                tracing::error!(?s, "failed to parse line number: {:?}", e);
                e
            })?))
        }
    }
}

impl LinesRange {
    fn parse(value: &str) -> Result<Vec<Self>, Error> {
        let mut lines = Vec::new();
        if value.contains(';') {
            lines.extend(
                value
                    .split(';')
                    .map(LinesRange::from_str)
                    .collect::<Result<Vec<_>, _>>()?,
            );
        } else if value.contains(',') {
            lines.extend(
                value
                    .split(',')
                    .map(LinesRange::from_str)
                    .collect::<Result<Vec<_>, _>>()?,
            );
        } else {
            lines.push(LinesRange::from_str(value)?);
        }
        Ok(lines)
    }
}

impl Include {
    fn parse_attributes(&mut self, attributes: Vec<(String, String)>) -> Result<(), Error> {
        for (key, value) in attributes {
            match key.as_ref() {
                "leveloffset" => {
                    self.level_offset = Some(
                        value
                            .parse()
                            .map_err(|_| Error::InvalidLevelOffset(value.to_string()))?,
                    );
                }
                "lines" => {
                    self.lines.extend(LinesRange::parse(&value).map_err(|e| {
                        tracing::error!(?value, "failed to parse lines attribute: {:?}", e);
                        e
                    })?);
                }
                "tag" => {
                    self.tags.push(value.to_string());
                }
                "tags" => {
                    self.tags.extend(value.split(';').map(str::to_string));
                }
                "indent" => {
                    self.indent = Some(
                        value
                            .parse()
                            .map_err(|_| Error::InvalidIndent(value.to_string()))?,
                    );
                }
                "encoding" => {
                    self.encoding = Some(value.to_string());
                }
                "opts" => {
                    self.opts.extend(value.split(',').map(str::to_string));
                }
                unknown => {
                    tracing::error!(?unknown, "unknown attribute key in include directive");
                    return Err(Error::InvalidIncludeDirective);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn parse(
        file_parent: &Path,
        line: &str,
        attributes: &DocumentAttributes,
    ) -> Result<Self, Error> {
        include_parser::include(line, file_parent, attributes).map_err(|e| {
            tracing::error!(?line, "failed to parse include directive: {:?}", e);
            Error::Parse(e.to_string())
        })?
    }

    pub(crate) fn read_content_from_file(&self, file_path: &Path) -> Result<String, Error> {
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
            // *preprocessor conditionals (e.g., ifdef)
            //
            // Running the preprocessor on the included content allows includes to be nested, thus
            // provides lot of flexibility in constructing radically different documents with a single
            // primary document and a few command line attributes.
            if ["adoc", "asciidoc", "ad", "asc", "txt"].contains(&ext.to_string_lossy().as_ref()) {
                return super::Preprocessor
                    .process_file(file_path, Some(&self.attributes.clone()))
                    .map_err(|e| {
                        tracing::error!(path=?file_path, error=?e, "failed to process file");
                        e
                    });
            }
        }
        Ok(std::fs::read_to_string(file_path).map_err(|e| {
            tracing::error!(path=?file_path, error=?e, "failed to read file");
            e
        })?)
    }

    pub(crate) fn lines(&self) -> Result<Vec<String>, Error> {
        // TODO(nlopes): need to read the file according to the properties of the include directive.
        //
        // Right now, this is a simplified version that reads the file as is.
        let mut lines = Vec::new();
        match &self.target {
            Target::Path(path) => {
                let path = self.file_parent.join(path);
                let content = self.read_content_from_file(&path)?;

                if let Some(level_offset) = self.level_offset {
                    tracing::warn!(level_offset, "level offset is not supported yet");
                }
                if !self.tags.is_empty() {
                    tracing::warn!(tags = ?self.tags, "tags are not supported yet");
                }
                if let Some(indent) = self.indent {
                    tracing::warn!(indent, "indent is not supported yet");
                }
                if let Some(encoding) = &self.encoding {
                    tracing::warn!(encoding, "encoding is not supported yet");
                }
                if !self.opts.is_empty() {
                    tracing::warn!(opts = ?self.opts, "opts are not supported yet");
                }
                // TODO(nlopes): this is so unoptimized, it isn't even funny but I'm
                // trying to just get to a place of compatibility, then I can
                // optimize.
                let content_lines = content.lines().map(str::to_string).collect::<Vec<_>>();
                if self.lines.is_empty() {
                    lines.extend(content_lines);
                } else {
                    for line in &self.lines {
                        match line {
                            LinesRange::Single(line_number) => {
                                if *line_number < 1 {
                                    // Skip invalid line numbers
                                    tracing::warn!(
                                        ?line_number,
                                        "invalid line number in include directive"
                                    );
                                    continue;
                                }
                                let line_number = line_number - 1;
                                if line_number < content_lines.len() {
                                    lines.push(content_lines[line_number].clone());
                                }
                            }
                            LinesRange::Range(start, end) => {
                                let raw_size = content_lines.len();
                                if *start < 1 {
                                    // Skip invalid line numbers
                                    tracing::warn!(
                                        ?start,
                                        "invalid start line number in include directive"
                                    );
                                    continue;
                                }
                                let start = *start - 1;
                                let end = if *end == -1 {
                                    raw_size
                                } else if *end > 0 {
                                    match (*end - 1).try_into() {
                                        Ok(end) => end,
                                        Err(e) => {
                                            tracing::error!(
                                                ?end,
                                                "failed to cast end line number to usize: {:?}",
                                                e
                                            );
                                            continue;
                                        }
                                    }
                                } else {
                                    // Skip invalid line numbers
                                    tracing::error!(
                                        ?end,
                                        "invalid end line number in include directive"
                                    );
                                    continue;
                                };
                                if start < raw_size && end < raw_size {
                                    for line in &content_lines[start..=end] {
                                        lines.push(line.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Target::Url(_url) => {
                if self.attributes.get("allow-uri-read").is_none() {
                    tracing::warn!("URL includes are disabled by default. If you want to enable them, set the 'allow-uri-read' attribute to 'true' in the document attributes or in the command line.");
                    return Ok(lines);
                }
                todo!("URL includes are not supported yet");
            }
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_simple_include() {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[]";
        let include = Include::parse(&path, line, &DocumentAttributes::default()).unwrap();

        match include.target {
            Target::Path(p) => assert_eq!(p, PathBuf::from("target.adoc")),
            Target::Url(_) => panic!("Expected Path target"),
        }
    }

    #[test]
    fn test_parse_include_with_attributes() {
        let path = PathBuf::from("/tmp");
        let line = "include::target.adoc[leveloffset=+1,lines=1..5,tag=example]";
        let include = Include::parse(&path, line, &DocumentAttributes::default()).unwrap();

        assert_eq!(include.level_offset, Some(1));
        assert_eq!(include.tags, vec!["example"]);
        assert!(!include.lines.is_empty());
    }

    #[test]
    fn test_parse_include_with_url() {
        let path = PathBuf::from("/tmp");
        let line = "include::https://example.com/doc.adoc[]";
        let include = Include::parse(&path, line, &DocumentAttributes::default()).unwrap();

        match include.target {
            Target::Url(url) => assert_eq!(url.as_str(), "https://example.com/doc.adoc"),
            Target::Path(_) => panic!("Expected URL target"),
        }
    }

    #[test]
    fn test_parse_quoted_attributes() {
        let path = PathBuf::from("/tmp");
        let line = r#"include::target.adoc[tag="example code",encoding="utf-8"]"#;
        let include = Include::parse(&path, line, &DocumentAttributes::default()).unwrap();

        assert_eq!(include.tags, vec!["example code"]);
        assert_eq!(include.encoding, Some("utf-8".to_string()));
    }
}
