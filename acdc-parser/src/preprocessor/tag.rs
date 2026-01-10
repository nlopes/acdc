pub(crate) const DELIMITERS: [char; 2] = [';', ','];

/// Represents a parsed tag filter from the `tag=` or `tags=` attribute.
///
/// Tag filters can be:
/// - A simple tag name: selects regions with that tag
/// - Negated (`!tag`): excludes regions with that tag
/// - Wildcard (`*`): selects all tagged regions
/// - Double wildcard (`**`): selects all lines except tag directive lines
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Filter {
    /// Select regions with this specific tag
    Include(String),
    /// Exclude regions with this specific tag
    Exclude(String),
    /// Select all tagged regions
    Wildcard,
    /// Select all lines except tag directive lines
    DoubleWildcard,
}

impl Filter {
    pub(crate) fn parse(tag: &str) -> Self {
        let tag = tag.trim();
        if tag == "**" {
            Filter::DoubleWildcard
        } else if tag == "*" {
            Filter::Wildcard
        } else if let Some(stripped) = tag.strip_prefix('!') {
            if stripped == "*" {
                // !* means select non-tagged regions (lines not in any tag)
                Filter::Exclude("*".to_string())
            } else {
                Filter::Exclude(stripped.to_string())
            }
        } else {
            Filter::Include(tag.to_string())
        }
    }
}

#[derive(Debug, PartialEq, Hash, Eq)]
pub(crate) struct Name(String);

impl Name {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for Name {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for Name {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A tagged region found in the content, with start and end line indices.
#[derive(Debug, PartialEq)]
pub(crate) struct Region {
    /// The tag name
    name: Name,
    /// Start line index (0-based, inclusive) - the line AFTER the tag directive
    start: usize,
    /// End line index (0-based, exclusive) - the line OF the end directive
    end: usize,
}

/// Extracts a tag name from a line if it contains a tag directive.
///
/// Returns `Some((directive_type, tag_name))` where `directive_type` is "tag" or "end",
/// or `None` if no valid tag directive is found.
///
/// Tag directives must follow a word boundary (preceded by non-alphanumeric or start of string)
/// and the tag name must consist of non-space, non-bracket characters.
fn extract_tag_directive(line: &str) -> Option<(&'static str, Name)> {
    // Look for "tag::" or "end::"
    for (directive, keyword) in [("tag", "tag::"), ("end", "end::")] {
        if let Some(pos) = line.find(keyword) {
            // Check word boundary: must be at start or preceded by non-alphanumeric
            if pos > 0 {
                let prev_char = line[..pos].chars().last();
                if prev_char.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
            }

            // Extract the tag name (everything after "::" until "[]")
            let after_keyword = &line[pos + keyword.len()..];
            if let Some(bracket_pos) = after_keyword.find("[]") {
                let tag_name = &after_keyword[..bracket_pos];
                // Tag name must not be empty and must not contain spaces or brackets
                if !tag_name.is_empty()
                    && !tag_name
                        .chars()
                        .any(|c| c.is_whitespace() || c == '[' || c == ']')
                {
                    return Some((directive, Name(tag_name.to_string())));
                }
            }
        }
    }
    None
}

/// Finds all tag regions in the content.
///
/// Tag regions are marked by `tag::name[]` and `end::name[]` directives.
/// The directives can appear after comment markers (e.g., `// tag::name[]`).
fn find_tag_regions(lines: &[String]) -> Vec<Region> {
    use rustc_hash::FxHashMap;

    let mut regions = Vec::new();
    let mut open_tags: FxHashMap<Name, usize> = FxHashMap::default();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some((directive, tag_name)) = extract_tag_directive(line) {
            match directive {
                "tag" => {
                    // Store the line AFTER the tag directive as the start
                    open_tags.insert(tag_name, line_idx + 1);
                }
                "end" => {
                    if let Some(start) = open_tags.remove(&tag_name) {
                        regions.push(Region {
                            name: tag_name,
                            start,
                            end: line_idx,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    // Warn about unclosed tags
    for (tag_name, _start_line) in open_tags {
        tracing::warn!(tag = %tag_name, "unclosed tag region");
    }

    regions
}

/// Checks if a line contains a tag directive (start or end).
fn is_tag_directive_line(line: &str) -> bool {
    extract_tag_directive(line).is_some()
}

/// Applies tag filters to select lines from content.
///
/// Returns the indices of lines that should be included.
pub(crate) fn apply_tag_filters(lines: &[String], filters: &[Filter]) -> Vec<usize> {
    let regions = find_tag_regions(lines);

    // Check for double wildcard - it has special priority and is always applied first
    let has_double_wildcard = filters.iter().any(|f| matches!(f, Filter::DoubleWildcard));

    if has_double_wildcard {
        // Select all lines except tag directive lines
        return (0..lines.len())
            .filter(|&i| lines.get(i).is_none_or(|line| !is_tag_directive_line(line)))
            .collect();
    }

    // Collect include and exclude filters
    let mut include_tags: Vec<&str> = Vec::new();
    let mut exclude_tags: Vec<&str> = Vec::new();
    let mut select_all_tagged = false;
    let mut select_untagged = false;

    for filter in filters {
        match filter {
            Filter::Include(name) => include_tags.push(name),
            Filter::Exclude(name) => {
                if name == "*" {
                    select_untagged = true;
                } else {
                    exclude_tags.push(name);
                }
            }
            Filter::Wildcard => select_all_tagged = true,
            // DoubleWildcard is already handled above with early return, this is defensive
            Filter::DoubleWildcard => {}
        }
    }

    // Build a set of line indices that are in each tagged region
    let mut tagged_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut selected_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for region in &regions {
        for i in region.start..region.end {
            tagged_lines.insert(i);
        }

        // Check if this region should be included
        let should_include = if select_all_tagged {
            !exclude_tags.contains(&region.name.as_str())
        } else {
            include_tags.contains(&region.name.as_str())
                && !exclude_tags.contains(&region.name.as_str())
        };

        if should_include {
            for i in region.start..region.end {
                selected_lines.insert(i);
            }
        }
    }

    // If select_untagged (!*), add lines that are not in any tagged region
    if select_untagged {
        for i in 0..lines.len() {
            if !tagged_lines.contains(&i) {
                selected_lines.insert(i);
            }
        }
    }

    // Filter out tag directive lines and sort
    let mut result: Vec<usize> = selected_lines
        .into_iter()
        .filter(|&i| lines.get(i).is_none_or(|line| !is_tag_directive_line(line)))
        .collect();
    result.sort_unstable();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_filter_parse_simple() {
        assert_eq!(Filter::parse("intro"), Filter::Include("intro".to_string()));
        assert_eq!(
            Filter::parse("my-tag"),
            Filter::Include("my-tag".to_string())
        );
        assert_eq!(
            Filter::parse("tag_123"),
            Filter::Include("tag_123".to_string())
        );
    }

    #[test]
    fn test_tag_filter_parse_negated() {
        assert_eq!(
            Filter::parse("!intro"),
            Filter::Exclude("intro".to_string())
        );
        assert_eq!(
            Filter::parse("!my-tag"),
            Filter::Exclude("my-tag".to_string())
        );
    }

    #[test]
    fn test_tag_filter_parse_wildcards() {
        assert_eq!(Filter::parse("*"), Filter::Wildcard);
        assert_eq!(Filter::parse("**"), Filter::DoubleWildcard);
        assert_eq!(Filter::parse("!*"), Filter::Exclude("*".to_string()));
    }

    #[test]
    fn test_tag_filter_parse_with_whitespace() {
        assert_eq!(
            Filter::parse("  intro  "),
            Filter::Include("intro".to_string())
        );
        assert_eq!(
            Filter::parse("  !intro  "),
            Filter::Exclude("intro".to_string())
        );
    }

    #[test]
    fn test_extract_tag_directive_simple() {
        assert_eq!(
            extract_tag_directive("// tag::intro[]"),
            Some(("tag", Name::from("intro")))
        );
        assert_eq!(
            extract_tag_directive("// end::intro[]"),
            Some(("end", Name::from("intro")))
        );
    }

    #[test]
    fn test_extract_tag_directive_various_comment_styles() {
        // C-style
        assert_eq!(
            extract_tag_directive("/* tag::example[] */"),
            Some(("tag", Name::from("example")))
        );
        // Hash comments
        assert_eq!(
            extract_tag_directive("# tag::ruby[]"),
            Some(("tag", Name::from("ruby")))
        );
        // XML-style
        assert_eq!(
            extract_tag_directive("<!-- tag::xml[] -->"),
            Some(("tag", Name::from("xml")))
        );
    }

    #[test]
    fn test_extract_tag_directive_at_line_start() {
        assert_eq!(
            extract_tag_directive("tag::start[]"),
            Some(("tag", Name::from("start")))
        );
        assert_eq!(
            extract_tag_directive("end::start[]"),
            Some(("end", Name::from("start")))
        );
    }

    #[test]
    fn test_extract_tag_directive_word_boundary() {
        // Should NOT match when preceded by alphanumeric
        assert_eq!(extract_tag_directive("atag::intro[]"), None);
        assert_eq!(extract_tag_directive("1tag::intro[]"), None);
        // Should match when preceded by non-alphanumeric
        assert_eq!(
            extract_tag_directive("-tag::intro[]"),
            Some(("tag", Name::from("intro")))
        );
        assert_eq!(
            extract_tag_directive(".tag::intro[]"),
            Some(("tag", Name::from("intro")))
        );
    }

    #[test]
    fn test_extract_tag_directive_complex_names() {
        assert_eq!(
            extract_tag_directive("// tag::my-complex_tag.name[]"),
            Some(("tag", Name::from("my-complex_tag.name")))
        );
    }

    #[test]
    fn test_extract_tag_directive_invalid() {
        // Empty tag name
        assert_eq!(extract_tag_directive("// tag::[]"), None);
        // Missing brackets
        assert_eq!(extract_tag_directive("// tag::intro"), None);
        // Space in tag name
        assert_eq!(extract_tag_directive("// tag::my tag[]"), None);
    }

    #[test]
    fn test_find_tag_regions_single() {
        let lines: Vec<String> = vec![
            "// tag::intro[]".to_string(),
            "This is the introduction.".to_string(),
            "// end::intro[]".to_string(),
        ];
        let regions = find_tag_regions(&lines);
        assert_eq!(
            regions,
            vec![Region {
                name: Name::from("intro"),
                start: 1,
                end: 2
            }]
        );
    }

    #[test]
    fn test_find_tag_regions_multiple() {
        let lines: Vec<String> = vec![
            "// tag::intro[]".to_string(),
            "Introduction content.".to_string(),
            "// end::intro[]".to_string(),
            String::new(),
            "// tag::main[]".to_string(),
            "Main content.".to_string(),
            "// end::main[]".to_string(),
        ];
        let regions = find_tag_regions(&lines);
        assert_eq!(regions.len(), 2);
    }

    #[test]
    fn test_find_tag_regions_nested() {
        let lines: Vec<String> = vec![
            "// tag::outer[]".to_string(),
            "Outer start.".to_string(),
            "// tag::inner[]".to_string(),
            "Inner content.".to_string(),
            "// end::inner[]".to_string(),
            "Outer end.".to_string(),
            "// end::outer[]".to_string(),
        ];
        let regions = find_tag_regions(&lines);
        // Inner tag should be found - use match to access safely
        let inner_region: Vec<Region> = regions
            .into_iter()
            .filter(|r| r.name == Name::from("inner"))
            .collect();
        assert_eq!(
            inner_region,
            vec![Region {
                name: Name::from("inner"),
                start: 3,
                end: 4
            }]
        );
    }

    #[test]
    fn test_apply_tag_filters_single_tag() {
        let lines: Vec<String> = vec![
            "Before tag.".to_string(),
            "// tag::intro[]".to_string(),
            "Introduction line 1.".to_string(),
            "Introduction line 2.".to_string(),
            "// end::intro[]".to_string(),
            "After tag.".to_string(),
        ];
        let filters = vec![Filter::Include("intro".to_string())];
        let selected = apply_tag_filters(&lines, &filters);
        assert_eq!(selected, vec![2, 3]); // Only content lines, not directives
    }

    #[test]
    fn test_apply_tag_filters_multiple_tags() {
        let lines: Vec<String> = vec![
            "// tag::intro[]".to_string(),
            "Intro.".to_string(),
            "// end::intro[]".to_string(),
            "// tag::main[]".to_string(),
            "Main.".to_string(),
            "// end::main[]".to_string(),
        ];
        let filters = vec![
            Filter::Include("intro".to_string()),
            Filter::Include("main".to_string()),
        ];
        let selected = apply_tag_filters(&lines, &filters);
        assert_eq!(selected, vec![1, 4]);
    }

    #[test]
    fn test_apply_tag_filters_wildcard() {
        let lines: Vec<String> = vec![
            "Untagged.".to_string(),
            "// tag::intro[]".to_string(),
            "Intro.".to_string(),
            "// end::intro[]".to_string(),
            "// tag::main[]".to_string(),
            "Main.".to_string(),
            "// end::main[]".to_string(),
            "More untagged.".to_string(),
        ];
        let filters = vec![Filter::Wildcard];
        let selected = apply_tag_filters(&lines, &filters);
        assert_eq!(selected, vec![2, 5]); // All tagged content, no untagged
    }

    #[test]
    fn test_apply_tag_filters_double_wildcard() {
        let lines: Vec<String> = vec![
            "Untagged line.".to_string(),
            "// tag::intro[]".to_string(),
            "Intro.".to_string(),
            "// end::intro[]".to_string(),
            "Another untagged.".to_string(),
        ];
        let filters = vec![Filter::DoubleWildcard];
        let selected = apply_tag_filters(&lines, &filters);
        // Should include all lines EXCEPT tag directive lines
        assert_eq!(selected, vec![0, 2, 4]);
    }

    #[test]
    fn test_apply_tag_filters_negation() {
        let lines: Vec<String> = vec![
            "// tag::intro[]".to_string(),
            "Intro.".to_string(),
            "// end::intro[]".to_string(),
            "// tag::main[]".to_string(),
            "Main.".to_string(),
            "// end::main[]".to_string(),
        ];
        // Select all tagged regions except "intro"
        let filters = vec![Filter::Wildcard, Filter::Exclude("intro".to_string())];
        let selected = apply_tag_filters(&lines, &filters);
        assert_eq!(selected, vec![4]); // Only main, not intro
    }

    #[test]
    fn test_apply_tag_filters_select_untagged() {
        let lines: Vec<String> = vec![
            "Untagged 1.".to_string(),
            "// tag::intro[]".to_string(),
            "Tagged.".to_string(),
            "// end::intro[]".to_string(),
            "Untagged 2.".to_string(),
        ];
        // !* selects non-tagged regions
        let filters = vec![Filter::Exclude("*".to_string())];
        let selected = apply_tag_filters(&lines, &filters);
        assert_eq!(selected, vec![0, 4]); // Only untagged lines
    }
}
