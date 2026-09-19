use acdc_parser::{AttributeValue, BlockMetadata};

/// Line-oriented rendering options for a source block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceLineOptions {
    /// The visible number assigned to the first source line, or `None` when
    /// line numbers are disabled.
    pub line_number_start: Option<usize>,
    /// Physical, one-based source lines that should be highlighted.
    pub highlighted_lines: Vec<usize>,
}

impl SourceLineOptions {
    /// Resolve line numbers and highlighted lines from source block metadata.
    ///
    /// `highlight` values use visible line numbers when numbering is enabled
    /// with a custom `start`; otherwise they use physical one-based lines.
    #[must_use]
    pub fn resolve(metadata: &BlockMetadata<'_>, source: &str) -> Self {
        if metadata.style != Some("source") {
            return Self::default();
        }
        let numbered =
            metadata.attributes.contains_key("linenums") || metadata.options.contains(&"linenums");
        let line_number_start = numbered.then(|| {
            string_attribute(metadata, "start")
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(1)
        });
        let highlighted_lines = string_attribute(metadata, "highlight")
            .map_or_else(Vec::new, |spec| {
                resolve_highlighted_lines(source, spec, line_number_start.unwrap_or(1))
            });

        Self {
            line_number_start,
            highlighted_lines,
        }
    }

    /// Whether neither line numbers nor highlighted lines are active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.line_number_start.is_none() && self.highlighted_lines.is_empty()
    }
}

fn string_attribute<'a>(metadata: &'a BlockMetadata<'_>, name: &str) -> Option<&'a str> {
    match metadata.attributes.get(name) {
        Some(AttributeValue::String(value)) => Some(value),
        _ => None,
    }
}

fn resolve_highlighted_lines(source: &str, spec: &str, start: usize) -> Vec<usize> {
    let line_count = source_line_count(source);
    let shift = start.saturating_sub(1) as i128;
    let spec = spec.replace(' ', "");
    let entries = if spec.contains(',') {
        spec.split(',')
    } else {
        spec.split(';')
    };
    let mut lines = vec![false; line_count.saturating_add(1)];

    for entry in entries {
        let (entry, exclude) = entry
            .strip_prefix('!')
            .map_or((entry, false), |entry| (entry, true));
        let range = entry.split_once("..").or_else(|| entry.split_once('-'));
        if let Some((from, to)) = range {
            let from = parse_selector_number(from) - shift;
            let parsed_to = parse_selector_number(to);
            let to = if to.is_empty() || parsed_to < 0 {
                line_count as i128
            } else {
                parsed_to
            } - shift;
            let first = from.max(1);
            let last = to.min(line_count as i128);
            if first <= last {
                for line in first..=last {
                    if let Ok(line) = usize::try_from(line) {
                        update_highlighted_line(&mut lines, line, exclude);
                    }
                }
            }
        } else {
            let line = parse_selector_number(entry) - shift;
            if (1..=line_count as i128).contains(&line)
                && let Ok(line) = usize::try_from(line)
            {
                update_highlighted_line(&mut lines, line, exclude);
            }
        }
    }

    lines
        .into_iter()
        .enumerate()
        .skip(1)
        .filter_map(|(line, highlighted)| highlighted.then_some(line))
        .collect()
}

/// Return the number of source lines that can receive line options.
///
/// Trailing empty lines are not counted because Asciidoctor removes them
/// before it applies line numbers and highlights.
#[must_use]
pub fn source_line_count(source: &str) -> usize {
    let source = source.trim_end_matches('\n');
    if source.is_empty() {
        0
    } else {
        source.split('\n').count()
    }
}

fn parse_selector_number(value: &str) -> i128 {
    value.parse().unwrap_or(0)
}

fn update_highlighted_line(lines: &mut [bool], line: usize, exclude: bool) {
    if let Some(highlighted) = lines.get_mut(line) {
        *highlighted = !exclude;
    }
}

/// Detect programming language from block metadata.
///
/// Returns the language if:
/// - The block has `style="source"`
/// - The metadata contains a `language` attribute
///
/// Any language string is returned, not just known ones. This ensures
/// `[source,text]` and other arbitrary languages get proper `<code>` wrappers.
#[must_use]
pub fn detect_language<'a, 'b: 'a>(metadata: &'a BlockMetadata<'b>) -> Option<&'a str> {
    let is_source = metadata.style == Some("source");
    if !is_source {
        return None;
    }

    metadata.attributes.get("language").and_then(|value| {
        if let acdc_parser::AttributeValue::String(value) = value {
            Some(value.as_ref())
        } else {
            None
        }
    })
}

/// Get the default line comment prefix for a programming language.
/// Used for stripping comment guards from callout markers in source blocks.
#[must_use]
pub fn default_line_comment(language: Option<&str>) -> Option<&'static str> {
    match language {
        // Hash comments
        Some(
            "python" | "py" | "ruby" | "rb" | "perl" | "bash" | "shell" | "sh" | "zsh" | "fish"
            | "console" | "terminal" | "powershell" | "ps1" | "yaml" | "yml" | "toml"
            | "dockerfile" | "makefile" | "cmake",
        ) => Some("#"),
        // Double-dash comments (SQL, Lua)
        Some("sql" | "lua") => Some("--"),
        // Semicolon comments
        Some("clojure" | "ini") => Some(";"),
        // XML/HTML comments are multiline, so we return None
        Some("html" | "xml" | "css" | "json") => None,
        // Default: assume C-style (//) for unknown languages and common C-family languages
        _ => Some("//"),
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use acdc_parser::{AttributeValue, BlockMetadata, ElementAttributes};

    use super::SourceLineOptions;

    fn metadata(attributes: &[(&'static str, &'static str)]) -> BlockMetadata<'static> {
        let mut values = ElementAttributes::default();
        for (name, value) in attributes {
            values.set(
                Cow::Borrowed(*name),
                AttributeValue::String(Cow::Borrowed(*value)),
            );
        }
        BlockMetadata::new()
            .with_style(Some("source"))
            .with_attributes(values)
    }

    #[test]
    fn resolves_numbering_start_and_visible_highlight_numbers() {
        let metadata = metadata(&[("linenums", ""), ("start", "10"), ("highlight", "10;12")]);

        assert_eq!(
            SourceLineOptions::resolve(&metadata, "ten\neleven\ntwelve"),
            SourceLineOptions {
                line_number_start: Some(10),
                highlighted_lines: vec![1, 3],
            }
        );
    }

    #[test]
    fn resolves_ranges_exclusions_and_open_ends() {
        let metadata = metadata(&[("highlight", "1..,!2,4")]);

        assert_eq!(
            SourceLineOptions::resolve(&metadata, "one\ntwo\nthree\nfour").highlighted_lines,
            [1, 3, 4]
        );
    }

    #[test]
    fn resolves_option_line_number_form() {
        let options = BlockMetadata::new()
            .with_style(Some("source"))
            .with_options(vec!["linenums"]);
        assert_eq!(
            SourceLineOptions::resolve(&options, "source").line_number_start,
            Some(1)
        );
    }

    #[test]
    fn source_line_count_ignores_trailing_empty_lines() {
        assert_eq!(super::source_line_count("one\n\ntwo\n\n"), 3);
        assert_eq!(super::source_line_count(""), 0);
    }
}
