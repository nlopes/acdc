use std::collections::HashSet;

use rustc_hash::FxHashMap;

/// One requested tag selection from `tag=` or `tags=`.
///
/// `selected = true` includes the tag; `false` excludes it. The names `*` and
/// `**` retain their Asciidoctor wildcard meanings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Filter {
    name: String,
    selected: bool,
}

impl Filter {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() || value == "!" {
            return None;
        }
        if let Some(name) = value.strip_prefix('!') {
            Some(Self {
                name: name.to_string(),
                selected: false,
            })
        } else {
            Some(Self {
                name: value.to_string(),
                selected: true,
            })
        }
    }
}

/// A recoverable problem discovered while selecting tagged lines.
///
/// These are scanner facts rather than parser diagnostics: `Include` owns the
/// directive location and target description and converts each issue into the
/// existing `Warning` representation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Issue {
    UnexpectedEnd {
        name: String,
        line: usize,
    },
    MismatchedEnd {
        expected: String,
        found: String,
        line: usize,
    },
    Unclosed {
        name: String,
        line: usize,
    },
    Missing {
        names: Vec<String>,
    },
}

/// Extract a tag marker from one source line.
///
/// Returns `("tag", name)` for an opening marker and `("end", name)` for a
/// closing marker. The marker may follow a comment prefix or another
/// non-word-boundary character.
fn extract_tag_directive(line: &str) -> Option<(&'static str, &str)> {
    for (directive, keyword) in [("tag", "tag::"), ("end", "end::")] {
        if let Some(pos) = line.find(keyword) {
            if pos > 0
                && line[..pos]
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }

            let after_keyword = &line[pos + keyword.len()..];
            if let Some(bracket_pos) = after_keyword.find("[]") {
                let tag_name = &after_keyword[..bracket_pos];
                if !tag_name.is_empty()
                    && !tag_name
                        .chars()
                        .any(|c| c.is_whitespace() || c == '[' || c == ']')
                {
                    return Some((directive, tag_name));
                }
            }
        }
    }
    None
}

pub(crate) fn is_tag_directive_line(line: &str) -> bool {
    extract_tag_directive(line).is_some()
}

/// Select source-line indices with Asciidoctor's tag-stack state machine.
///
/// Marker lines are never selected. Recoverable malformed-boundary and
/// missing-selection issues are reported in reference order through `report`.
pub(crate) fn select_tagged_lines(
    lines: &[String],
    filters: &[Filter],
    mut report: impl FnMut(Issue),
) -> Vec<usize> {
    // Ruby Hash insertion order is stable and assigning a duplicate key updates
    // its value without moving it. Keep the order separately so missing-tag
    // diagnostics remain deterministic while duplicate selectors are last-wins.
    let mut order = Vec::new();
    let mut requested: FxHashMap<&str, bool> = FxHashMap::default();
    for filter in filters {
        if !requested.contains_key(filter.name.as_str()) {
            order.push(filter.name.as_str());
        }
        requested.insert(filter.name.as_str(), filter.selected);
    }

    let first_name = order.first().copied();
    let (mut select, base_select, wildcard) = if let Some(double_wildcard) = requested.remove("**")
    {
        let wildcard = requested.remove("*").or_else(|| {
            let first_remaining = order.iter().find_map(|name| requested.get(*name).copied());
            (!double_wildcard && first_remaining == Some(false)).then_some(true)
        });
        (double_wildcard, double_wildcard, wildcard)
    } else if let Some(wildcard) = requested.remove("*") {
        if first_name == Some("*") {
            (!wildcard, !wildcard, Some(wildcard))
        } else {
            (false, false, Some(wildcard))
        }
    } else {
        let base_select = !requested.values().any(|selected| *selected);
        (base_select, base_select, None)
    };

    // (tag name, selection state established by that tag, opening line)
    let mut tag_stack: Vec<(String, bool, usize)> = Vec::new();
    let mut selected_tags = HashSet::new();
    let mut selected_lines = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        let source_line = line_idx + 1;
        if let Some((directive, tag_name)) = extract_tag_directive(line) {
            if directive == "end" {
                if tag_stack
                    .last()
                    .is_some_and(|(active, _, _)| active == tag_name)
                {
                    tag_stack.pop();
                    select = tag_stack
                        .last()
                        .map_or(base_select, |(_, selected, _)| *selected);
                } else if requested.contains_key(tag_name) {
                    if let Some(idx) = tag_stack
                        .iter()
                        .rposition(|(open_name, _, _)| open_name == tag_name)
                    {
                        let expected = tag_stack
                            .last()
                            .map_or_else(String::new, |(name, _, _)| name.clone());
                        tag_stack.remove(idx);
                        report(Issue::MismatchedEnd {
                            expected,
                            found: tag_name.to_string(),
                            line: source_line,
                        });
                    } else {
                        report(Issue::UnexpectedEnd {
                            name: tag_name.to_string(),
                            line: source_line,
                        });
                    }
                }
            } else if let Some(tag_select) = requested.get(tag_name).copied() {
                select = tag_select;
                if select {
                    selected_tags.insert(tag_name.to_string());
                }
                tag_stack.push((tag_name.to_string(), select, source_line));
            } else if let Some(wildcard_select) = wildcard {
                select = if tag_stack.last().is_some() && !select {
                    false
                } else {
                    wildcard_select
                };
                tag_stack.push((tag_name.to_string(), select, source_line));
            }
            continue;
        }

        if select {
            selected_lines.push(line_idx);
        }
    }

    for (name, _, line) in tag_stack {
        report(Issue::Unclosed { name, line });
    }

    let missing = order
        .into_iter()
        .filter(|name| {
            requested.get(*name).copied() == Some(true) && !selected_tags.contains(*name)
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        report(Issue::Missing { names: missing });
    }

    selected_lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIXED_SELECTOR_LINES: &[&str] = &[
        "Untagged before.",
        "// tag::beta[]",
        "Beta.",
        "// end::beta[]",
        "// tag::alpha[]",
        "Alpha.",
        "// end::alpha[]",
        "// tag::other[]",
        "Other.",
        "// end::other[]",
        "Untagged after.",
    ];

    fn strings(lines: &[&str]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    fn filters(values: &[&str]) -> Vec<Filter> {
        values
            .iter()
            .filter_map(|value| Filter::parse(value))
            .collect()
    }

    fn select(lines: &[&str], values: &[&str]) -> (Vec<usize>, Vec<Issue>) {
        let lines = strings(lines);
        let mut issues = Vec::new();
        let selected = select_tagged_lines(&lines, &filters(values), |issue| issues.push(issue));
        (selected, issues)
    }

    #[test]
    fn parses_selected_and_excluded_filters() {
        assert_eq!(
            Filter::parse("intro"),
            Some(Filter {
                name: "intro".to_string(),
                selected: true,
            })
        );
        assert_eq!(
            Filter::parse(" !debug "),
            Some(Filter {
                name: "debug".to_string(),
                selected: false,
            })
        );
        assert_eq!(
            Filter::parse("**"),
            Some(Filter {
                name: "**".to_string(),
                selected: true,
            })
        );
        assert_eq!(Filter::parse("!"), None);
        assert_eq!(Filter::parse("  "), None);
    }

    #[test]
    fn extracts_valid_tag_markers() {
        assert_eq!(
            extract_tag_directive("// tag::intro[]"),
            Some(("tag", "intro"))
        );
        assert_eq!(
            extract_tag_directive("# end::my-tag[]"),
            Some(("end", "my-tag"))
        );
        assert_eq!(
            extract_tag_directive("tag::at-start[]"),
            Some(("tag", "at-start"))
        );
        assert_eq!(extract_tag_directive("notatag::intro[]"), None);
        assert_eq!(extract_tag_directive("// tag::has space[]"), None);
        assert_eq!(extract_tag_directive("// tag::[]"), None);
    }

    #[test]
    fn selects_one_requested_tag() {
        let (selected, issues) = select(
            &[
                "Before.",
                "// tag::intro[]",
                "Selected one.",
                "Selected two.",
                "// end::intro[]",
                "After.",
            ],
            &["intro"],
        );
        assert_eq!(selected, [2, 3]);
        assert!(issues.is_empty());
    }

    #[test]
    fn selects_multiple_requested_tags() {
        let (selected, issues) = select(
            &[
                "// tag::intro[]",
                "Introduction.",
                "// end::intro[]",
                "// tag::main[]",
                "Main.",
                "// end::main[]",
            ],
            &["intro", "main"],
        );
        assert_eq!(selected, [1, 4]);
        assert!(issues.is_empty());
    }

    #[test]
    fn wildcard_selects_tagged_content_except_exclusions() {
        let (selected, issues) = select(
            &[
                "Untagged.",
                "// tag::intro[]",
                "Introduction.",
                "// end::intro[]",
                "// tag::debug[]",
                "Debug.",
                "// end::debug[]",
            ],
            &["*", "!debug"],
        );
        assert_eq!(selected, [2]);
        assert!(issues.is_empty());
    }

    #[test]
    fn double_wildcard_selects_every_non_marker_line() {
        let (selected, issues) = select(
            &[
                "Before.",
                "// tag::intro[]",
                "Introduction.",
                "// end::intro[]",
                "After.",
            ],
            &["**"],
        );
        assert_eq!(selected, [0, 2, 4]);
        assert!(issues.is_empty());
    }

    #[test]
    fn excluded_wildcard_selects_only_untagged_content() {
        let (selected, issues) = select(
            &[
                "Before.",
                "// tag::intro[]",
                "Introduction.",
                "// end::intro[]",
                "After.",
            ],
            &["!*"],
        );
        assert_eq!(selected, [0, 4]);
        assert!(issues.is_empty());
    }

    #[test]
    fn negated_double_wildcard_uses_first_remaining_selector_order() {
        let (selected, issues) = select(MIXED_SELECTOR_LINES, &["!**", "beta", "!alpha"]);
        assert_eq!(selected, [2]);
        assert!(issues.is_empty());

        let (selected, issues) = select(MIXED_SELECTOR_LINES, &["!**", "!alpha", "beta"]);
        assert_eq!(selected, [2, 8]);
        assert!(issues.is_empty());
    }

    #[test]
    fn negated_double_wildcard_uses_final_value_at_first_insertion_position() {
        let (selected, issues) = select(MIXED_SELECTOR_LINES, &["!**", "!beta", "!alpha", "beta"]);
        assert_eq!(selected, [2]);
        assert!(issues.is_empty());

        let (selected, issues) = select(MIXED_SELECTOR_LINES, &["!**", "!alpha", "!beta", "beta"]);
        assert_eq!(selected, [2, 8]);
        assert!(issues.is_empty());
    }

    #[test]
    fn explicit_wildcard_overrides_negated_double_wildcard_inference() {
        let (selected, issues) = select(MIXED_SELECTOR_LINES, &["!**", "beta", "!alpha", "*"]);
        assert_eq!(selected, [2, 8]);
        assert!(issues.is_empty());

        let (selected, issues) = select(MIXED_SELECTOR_LINES, &["!**", "!alpha", "beta", "!*"]);
        assert_eq!(selected, [2]);
        assert!(issues.is_empty());
    }

    #[test]
    fn requested_outer_tag_keeps_unrequested_nested_content() {
        let (selected, issues) = select(
            &[
                "// tag::outer[]",
                "Outer A.",
                "// tag::inner[]",
                "Inner.",
                "// end::inner[]",
                "Outer B.",
                "// end::outer[]",
            ],
            &["outer"],
        );
        assert_eq!(selected, [1, 3, 5]);
        assert!(issues.is_empty());
    }

    #[test]
    fn nested_duplicate_names_use_a_stack() {
        let (selected, issues) = select(
            &[
                "// tag::same[]",
                "Outer A.",
                "// tag::same[]",
                "Inner.",
                "// end::same[]",
                "Outer B.",
                "// end::same[]",
            ],
            &["same"],
        );
        assert_eq!(selected, [1, 3, 5]);
        assert!(issues.is_empty());
    }

    #[test]
    fn mismatched_end_removes_matching_frame_and_keeps_active_tag() {
        let (selected, issues) = select(
            &[
                "// tag::outer[]",
                "Outer.",
                "// tag::inner[]",
                "Inner.",
                "// end::outer[]",
                "Still inner.",
                "// end::inner[]",
            ],
            &["outer", "inner"],
        );
        assert_eq!(selected, [1, 3, 5]);
        assert_eq!(
            issues,
            [Issue::MismatchedEnd {
                expected: "inner".to_string(),
                found: "outer".to_string(),
                line: 5,
            }]
        );
    }

    #[test]
    fn unexpected_end_is_reported() {
        let (selected, issues) = select(
            &[
                "// tag::wanted[]",
                "Selected.",
                "// end::wanted[]",
                "// end::wanted[]",
            ],
            &["wanted"],
        );
        assert_eq!(selected, [1]);
        assert_eq!(
            issues,
            [Issue::UnexpectedEnd {
                name: "wanted".to_string(),
                line: 4,
            }]
        );
    }

    #[test]
    fn unclosed_selected_tag_continues_through_end_of_file() {
        let (selected, issues) = select(&["// tag::wanted[]", "Selected."], &["wanted"]);
        assert_eq!(selected, [1]);
        assert_eq!(
            issues,
            [Issue::Unclosed {
                name: "wanted".to_string(),
                line: 1,
            }]
        );
    }

    #[test]
    fn missing_tags_preserve_requested_order_and_deduplicate() {
        let (selected, issues) = select(&["Plain."], &["alpha", "beta", "alpha"]);
        assert!(selected.is_empty());
        assert_eq!(
            issues,
            [Issue::Missing {
                names: vec!["alpha".to_string(), "beta".to_string()],
            }]
        );
    }

    #[test]
    fn duplicate_filter_values_are_last_wins() {
        let (selected, issues) = select(
            &[
                "// tag::wanted[]",
                "Selected.",
                "// end::wanted[]",
                "Plain.",
            ],
            &["wanted", "!wanted"],
        );
        assert_eq!(selected, [3]);
        assert!(issues.is_empty());
    }
}
