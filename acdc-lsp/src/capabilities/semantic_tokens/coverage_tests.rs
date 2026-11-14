use acdc_parser::{
    CompatibilityMode, IncludeLoader, IncludeSource, IncludeSourceError, IncludeSourceErrorKind,
    IncludeSourceTarget, Options, SafeMode,
};

use super::*;
use crate::state::{document::evaluated_conditionals, extract_includes};

type TestResult = Result<(), Box<dyn std::error::Error>>;
type HighlightedToken = (u32, u32, String, String, Vec<String>);

const MODES: [CompatibilityMode; 2] = [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft];

fn decoded(text: &str) -> Result<Vec<HighlightedToken>, Box<dyn std::error::Error>> {
    decoded_in(text, &Options::default())
}

/// Decode tokens with the conditionals and includes the workspace records.
fn decoded_in(
    text: &str,
    options: &Options<'_>,
) -> Result<Vec<HighlightedToken>, Box<dyn std::error::Error>> {
    let parsed = acdc_parser::parse(text, options)?;
    let conditionals = evaluated_conditionals(text, &parsed);
    let tokens = compute_semantic_tokens(
        parsed.document(),
        &conditionals,
        &extract_includes(text),
        text,
    );
    let mut line = 0;
    let mut column = 0;
    let mut previous_end = (0, 0);
    let mut result = Vec::new();
    for token in tokens.data {
        line += token.delta_line;
        column = if token.delta_line == 0 {
            column + token.delta_start
        } else {
            token.delta_start
        };
        assert!(token.length > 0);
        assert!((line, column) >= previous_end, "overlapping tokens");
        previous_end = (line, column + token.length);
        let text = text.lines().nth(line as usize).unwrap_or_default();
        let units: Vec<_> = text
            .encode_utf16()
            .skip(column as usize)
            .take(token.length as usize)
            .collect();
        let spelling = String::from_utf16(&units)?;
        assert_eq!(units.len(), token.length as usize);
        let kind = TOKEN_TYPES
            .get(token.token_type as usize)
            .map_or("unknown", SemanticTokenType::as_str);
        let modifiers = TOKEN_MODIFIERS
            .iter()
            .enumerate()
            .filter(|(bit, _)| token.token_modifiers_bitset & (1 << bit) != 0)
            .map(|(_, modifier)| modifier.as_str().to_owned())
            .collect();
        result.push((line, column, spelling, kind.to_owned(), modifiers));
    }
    Ok(result)
}

#[test]
fn table_separators_and_cell_content_use_original_spelling() -> TestResult {
    let source = "= Tables\r\n\r\n[%header,cols=\"1,1\"]\r\n|===\r\n|Name|Email\r\n|Peter Sutherland|peter.sutherland@example.com\r\n|😀 *Bold*|_Italic_\r\n|escaped \\| pipe|plain\r\n|===\r\n";
    let tokens = decoded(source)?;
    assert!(
        tokens
            .iter()
            .any(|token| token.2 == "|===" && token.3 == "operator")
    );
    for line in [4, 5, 6, 7] {
        assert_eq!(
            tokens
                .iter()
                .filter(|token| token.0 == line && token.2 == "|" && token.3 == "operator")
                .count(),
            2,
            "{tokens:?}"
        );
    }
    assert!(!tokens.iter().any(|token| token.2 == "e" || token.2 == "d"));
    assert!(
        tokens
            .iter()
            .any(|token| token.2 == "peter.sutherland@example.com" && token.3 == "function")
    );
    assert!(tokens.iter().any(|token| token.0 == 6
        && token.1 == 4
        && token.2 == "*Bold*"
        && token.4.contains(&"bold".to_owned())));
    assert!(
        tokens
            .iter()
            .any(|token| token.2 == "_Italic_" && token.4.contains(&"italic".to_owned()))
    );
    Ok(())
}

#[test]
fn block_syntax_and_multiline_content_receive_tokens() -> TestResult {
    let source = "= Document\n:author: Ada\n\n== Section\n\n.List title\n* [x] Item\n\nTerm:: Definition\n\n[source,rust]\n----\nfn main() {\n    println!(\"hello\");\n}\n----\n\n////\nfirst comment\nsecond comment\n////\n\nNOTE: Body text.\n";
    let tokens = decoded(source)?;
    for spelling in [
        "Document",
        "Section",
        "List title",
        "author",
        "Ada",
        "*",
        "[x]",
        "::",
        "[source,rust]",
        "----",
        "NOTE:",
    ] {
        assert!(
            tokens.iter().any(|token| token.2 == spelling),
            "missing {spelling:?}: {tokens:?}"
        );
    }
    for spelling in ["fn main() {", "    println!(\"hello\");", "}"] {
        assert!(
            tokens
                .iter()
                .any(|token| token.2 == spelling && token.4.contains(&"code".to_owned())),
            "{tokens:?}"
        );
    }
    for spelling in ["first comment", "second comment"] {
        assert!(
            tokens
                .iter()
                .any(|token| token.2 == spelling && token.3 == "comment"),
            "{tokens:?}"
        );
    }
    assert!(
        !tokens
            .iter()
            .any(|token| token.2.contains("Body text") && token.3 == "keyword")
    );
    Ok(())
}

#[test]
fn nested_and_multiline_styles_combine_without_overlapping_tokens() -> TestResult {
    let source =
        "*Bold _and italic_*\r\n\r\n*First line\r\nsecond line* and `code` and #marked#.\r\n";
    let tokens = decoded(source)?;
    assert!(
        tokens.iter().any(|token| token.2 == "_and italic_"
            && token.4.contains(&"bold".to_owned())
            && token.4.contains(&"italic".to_owned())),
        "{tokens:?}"
    );
    for spelling in ["*First line", "second line*"] {
        assert!(
            tokens
                .iter()
                .any(|token| token.2 == spelling && token.4.contains(&"bold".to_owned())),
            "{tokens:?}"
        );
    }
    assert!(
        tokens
            .iter()
            .any(|token| token.2 == "`code`" && token.4.contains(&"code".to_owned()))
    );
    assert!(
        tokens
            .iter()
            .any(|token| token.2 == "#marked#" && token.4.contains(&"highlight".to_owned()))
    );
    Ok(())
}

#[test]
fn csv_dsv_and_custom_separators_respect_quotes_and_escapes() -> TestResult {
    for (attributes, delimiter, first, second, separator) in [
        ("", ",===", "Name,Value", "\"comma, inside\",data", ","),
        (
            "[format=csv,separator=;]\n",
            "|===",
            "Name;Value",
            "\"semi; inside\";data",
            ";",
        ),
        ("", ":===", "Name:Value", "escaped \\: colon:data", ":"),
        (
            "[separator=!]\n",
            "|===",
            "Name!Value",
            "escaped \\! bang!data",
            "!",
        ),
    ] {
        let source = format!("{attributes}{delimiter}\n{first}\n{second}\n{delimiter}\n");
        let tokens = decoded(&source)?;
        assert_eq!(
            tokens
                .iter()
                .filter(|token| token.2 == separator && token.3 == "operator")
                .count(),
            2,
            "{source}: {tokens:?}"
        );
    }
    Ok(())
}

#[test]
fn table_column_styles_and_literal_substitutions_remain_distinct() -> TestResult {
    let source = "[cols=\"e,s,m\"]\n|===\n|Emphasis|Strong|Code\n|===\n\n----\n{attribute} *literal*\n----\n";
    let tokens = decoded(source)?;
    for (spelling, style) in [("Emphasis", "italic"), ("Strong", "bold"), ("Code", "code")] {
        assert!(
            tokens
                .iter()
                .any(|token| token.2 == spelling && token.4.contains(&style.to_owned())),
            "{tokens:?}"
        );
    }
    assert!(
        tokens.iter().any(|token| token.2 == "{attribute} *literal*"
            && token.3 == "string"
            && token.4 == ["code"]),
        "{tokens:?}"
    );
    Ok(())
}

#[test]
fn block_titles_keep_their_faces_separate_from_body_syntax() -> TestResult {
    let source = "= Doc\n\n.Image title\nimage::photo.png[Alt]\n\n[stem]\n.Math title\n++++\nx=y\n++++\n\n.Break title\n'''\n\n.Admonition title\nNOTE: text\n";
    let tokens = decoded(source)?;
    for spelling in [
        "Image title",
        "Math title",
        "Break title",
        "Admonition title",
    ] {
        assert!(
            tokens
                .iter()
                .any(|token| token.2 == spelling && token.3 == "namespace" && token.4.is_empty()),
            "{spelling}: {tokens:?}"
        );
    }
    assert!(
        tokens
            .iter()
            .any(|token| token.2 == "[stem]" && token.3 == "property" && token.4.is_empty()),
        "{tokens:?}"
    );
    assert!(
        tokens
            .iter()
            .any(|token| token.2 == "NOTE:" && token.3 == "keyword"),
        "{tokens:?}"
    );
    Ok(())
}

#[test]
fn leading_attribute_groups_before_title_are_all_highlighted() -> TestResult {
    let source = ":description: How to install the product.\n\n:doctype: book\n:toc: left\n\n\n:icons: font\n\n= User Guide\n\n== Install\n";
    let tokens = decoded(source)?;
    for (line, name, value) in [
        (0, "description", "How to install the product."),
        (2, "doctype", "book"),
        (3, "toc", "left"),
        (6, "icons", "font"),
    ] {
        assert!(
            tokens
                .iter()
                .any(|token| token.0 == line && token.2 == name && token.3 == "property"),
            "{name}: {tokens:?}"
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.0 == line && token.2 == value && token.3 == "string"),
            "{value}: {tokens:?}"
        );
    }
    Ok(())
}

#[test]
fn leading_attribute_scan_stops_at_body_content() -> TestResult {
    let tokens = decoded(":a: 1\n\nPara text\n:b: 2\n")?;
    assert!(
        !tokens
            .iter()
            .any(|token| token.0 >= 2 && matches!(token.3.as_str(), "property" | "string")),
        "{tokens:?}"
    );

    let tokens = decoded(":a: one \\\n\nPara text\n")?;
    assert!(
        !tokens
            .iter()
            .any(|token| token.0 == 2 && token.3 == "string"),
        "{tokens:?}"
    );
    Ok(())
}

#[test]
fn lone_backslash_value_does_not_continue_into_body() -> TestResult {
    let tokens = decoded(":value: \\\nFollowing paragraph.\n")?;
    assert!(
        !tokens
            .iter()
            .any(|token| token.0 == 1 && token.3 == "string"),
        "{tokens:?}"
    );

    let tokens = decoded(":value: \\\n----\n:x: y\n----\n")?;
    assert!(
        !tokens
            .iter()
            .any(|token| token.0 == 2 && token.3 == "property"),
        "{tokens:?}"
    );

    let tokens = decoded(":a: one \\\ntwo\n")?;
    assert!(
        tokens
            .iter()
            .any(|token| token.0 == 1 && token.2 == "two" && token.3 == "string"),
        "{tokens:?}"
    );
    Ok(())
}

#[test]
fn slash_prefixed_paragraph_ends_leading_attribute_scan() -> TestResult {
    for source in ["///\n:foo: bar\n", "////x\n:foo: bar\n"] {
        let tokens = decoded(source)?;
        assert!(
            !tokens
                .iter()
                .any(|token| token.0 == 1 && matches!(token.3.as_str(), "property" | "string")),
            "{source:?}: {tokens:?}"
        );
    }

    let tokens = decoded("// note\n:foo: bar\n")?;
    assert!(
        tokens
            .iter()
            .any(|token| token.0 == 1 && token.2 == "foo" && token.3 == "property"),
        "{tokens:?}"
    );
    Ok(())
}

#[test]
fn comment_delimiters_with_trailing_whitespace_hide_their_content() -> TestResult {
    for source in [
        ":a: 1\n//// \n:x: y\n////\n:b: 2\n\nPara\n",
        ":a: 1\n////\n:x: y\n//// \n:b: 2\n\nPara\n",
    ] {
        let tokens = decoded(source)?;
        assert!(
            !tokens
                .iter()
                .any(|token| token.0 == 2 && token.3 == "property"),
            "{source:?}: {tokens:?}"
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.0 == 4 && token.2 == "b" && token.3 == "property"),
            "{source:?}: {tokens:?}"
        );
    }
    Ok(())
}

#[test]
fn attribute_entries_need_a_name_and_separator() -> TestResult {
    for source in [":foo:bar\n:baz: qux\n", ":: x\n:baz: qux\n"] {
        let tokens = decoded(source)?;
        assert!(
            !tokens
                .iter()
                .any(|token| token.0 == 1 && matches!(token.3.as_str(), "property" | "string")),
            "{source:?}: {tokens:?}"
        );
    }

    let tokens = decoded(":foo:\n:b: 2\n")?;
    assert!(
        tokens
            .iter()
            .any(|token| token.0 == 1 && token.2 == "b" && token.3 == "property"),
        "{tokens:?}"
    );
    Ok(())
}

fn options(mode: CompatibilityMode) -> Result<Options<'static>, Box<dyn std::error::Error>> {
    with_includes(mode, &[])
}

/// Serve the named files to `include::` directives.
fn with_includes(
    mode: CompatibilityMode,
    files: &[(&'static str, &'static str)],
) -> Result<Options<'static>, Box<dyn std::error::Error>> {
    let files = files.to_vec();
    Ok(Options::builder()
        .with_compatibility_mode(mode)
        .with_safe_mode(SafeMode::Server)
        // Nothing is read from disk; the loader serves `files`.
        .with_base_dir(std::path::PathBuf::from("/acdc-lsp-tests"))
        .with_include_loader(IncludeLoader::custom(
            move |target: &IncludeSourceTarget| {
                if let IncludeSourceTarget::File(path) = target
                    && let Some((_, text)) = files
                        .iter()
                        .find(|(name, _)| path.file_name().is_some_and(|file| file == *name))
                {
                    return Ok(IncludeSource::from_string((*text).to_owned()));
                }
                Err(IncludeSourceError::new(
                    IncludeSourceErrorKind::NotFound,
                    "unknown test file",
                ))
            },
        ))
        .build()?)
}

fn has_token(tokens: &[HighlightedToken], line: u32, spelling: &str, kind: &str) -> bool {
    tokens
        .iter()
        .any(|token| token.0 == line && token.2 == spelling && token.3 == kind)
}

fn has_kind_on_line(tokens: &[HighlightedToken], line: u32, kind: &str) -> bool {
    tokens
        .iter()
        .any(|token| token.0 == line && token.3 == kind)
}

#[test]
fn header_entries_and_comments_are_highlighted_with_and_without_a_title() -> TestResult {
    for mode in MODES {
        for (source, offset) in [
            (
                ":a: 1\n// note\n:b: 2 \\\ncontinued\n////\n:x: y\n////\n:c: 3\n\nPara\n",
                0,
            ),
            (
                "= Title\n:a: 1\n// note\n:b: 2 \\\ncontinued\n////\n:x: y\n////\n:c: 3\n\nPara\n",
                1,
            ),
        ] {
            let tokens = decoded_in(source, &options(mode)?)?;
            let message = format!("{mode:?} {source:?}: {tokens:?}");
            assert!(has_token(&tokens, offset, "a", "property"), "{message}");
            assert!(
                has_token(&tokens, offset + 1, "// note", "comment"),
                "{message}"
            );
            assert!(has_token(&tokens, offset + 2, "b", "property"), "{message}");
            assert!(
                has_token(&tokens, offset + 3, "continued", "string"),
                "{message}"
            );
            assert!(
                has_token(&tokens, offset + 4, "////", "operator"),
                "{message}"
            );
            // A commented-out entry is a comment, not an attribute.
            assert!(
                has_token(&tokens, offset + 5, ":x: y", "comment"),
                "{message}"
            );
            assert!(
                !has_kind_on_line(&tokens, offset + 5, "property"),
                "{message}"
            );
            assert!(
                has_token(&tokens, offset + 6, "////", "operator"),
                "{message}"
            );
            assert!(has_token(&tokens, offset + 7, "c", "property"), "{message}");
            assert!(
                !has_kind_on_line(&tokens, offset + 9, "property"),
                "{message}"
            );
        }
    }
    Ok(())
}

#[test]
fn asciidoctor_header_spans_empty_lines_before_the_title() -> TestResult {
    let source = ":a: 1\n\n// note\n\n:b: 2\n\n= Title\n:c: 3\n\nPara\n";
    let tokens = decoded_in(source, &options(CompatibilityMode::Asciidoctor)?)?;
    assert!(has_token(&tokens, 0, "a", "property"), "{tokens:?}");
    assert!(has_token(&tokens, 2, "// note", "comment"), "{tokens:?}");
    assert!(has_token(&tokens, 4, "b", "property"), "{tokens:?}");
    assert!(has_token(&tokens, 6, "Title", "namespace"), "{tokens:?}");
    assert!(has_token(&tokens, 7, "c", "property"), "{tokens:?}");
    Ok(())
}

#[test]
fn trailing_header_comments_outside_the_span_are_highlighted() -> TestResult {
    for mode in MODES {
        for (source, line) in [
            (":a: 1\n// one\n// two\n\nPara\n", 1),
            ("= Title\n:a: 1\n// one\n// two\n\nPara\n", 2),
        ] {
            let tokens = decoded_in(source, &options(mode)?)?;
            assert!(
                has_token(&tokens, line, "// one", "comment"),
                "{mode:?}: {tokens:?}"
            );
            assert!(
                has_token(&tokens, line + 1, "// two", "comment"),
                "{mode:?}: {tokens:?}"
            );
            assert!(
                !has_kind_on_line(&tokens, line + 3, "comment"),
                "{mode:?}: {tokens:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn wrapped_values_are_highlighted_on_every_line() -> TestResult {
    for mode in MODES {
        for source in [
            "= Title\n:a: one \\\n  two\n\nPara\n",
            "= Title\n\n:a: one \\\n  two\n\nPara\n",
        ] {
            let tokens = decoded_in(source, &options(mode)?)?;
            let line = u32::try_from(
                source
                    .lines()
                    .position(|line| line.starts_with(":a:"))
                    .unwrap_or_default(),
            )?;
            assert!(
                has_token(&tokens, line, "a", "property"),
                "{mode:?} {source:?}: {tokens:?}"
            );
            assert!(
                has_kind_on_line(&tokens, line + 1, "string"),
                "{mode:?} {source:?}: {tokens:?}"
            );
            assert!(
                !has_kind_on_line(&tokens, line + 3, "string"),
                "{mode:?} {source:?}: {tokens:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn directive_lines_inside_the_header_do_not_end_it() -> TestResult {
    for mode in MODES {
        let source = ":a: 1\nifdef::a[]\n:b: 2\nendif::[]\n:c: 3\n\nPara\n";
        let tokens = decoded_in(source, &options(mode)?)?;
        for (line, name) in [(0, "a"), (2, "b"), (4, "c")] {
            assert!(
                has_token(&tokens, line, name, "property"),
                "{mode:?}: {tokens:?}"
            );
        }
        for line in [1, 3] {
            assert!(
                has_kind_on_line(&tokens, line, "keyword"),
                "{mode:?}: {tokens:?}"
            );
            assert!(
                !has_kind_on_line(&tokens, line, "property"),
                "{mode:?}: {tokens:?}"
            );
        }

        // An excluded comment delimiter cannot hide the entries after it.
        let source = "= Title\n:a: 1\nifdef::missing[]\n////\nendif::[]\n:b: 2\n\nPara\n";
        let tokens = decoded_in(source, &options(mode)?)?;
        assert!(
            !has_kind_on_line(&tokens, 3, "operator"),
            "{mode:?}: {tokens:?}"
        );
        assert!(
            has_token(&tokens, 5, "b", "property"),
            "{mode:?}: {tokens:?}"
        );
    }
    Ok(())
}

/// Whether the start and end of the header span are in included files.
fn span_in_includes(
    source: &str,
    options: &Options<'_>,
) -> Result<(bool, bool), Box<dyn std::error::Error>> {
    let parsed = acdc_parser::parse(source, options)?;
    let span = parsed.document().header_location().ok_or("no header")?;
    Ok((
        span.start().include_chain().is_some(),
        span.end().include_chain().is_some(),
    ))
}

#[test]
fn header_highlighting_stops_at_included_text() -> TestResult {
    let files = [
        ("attributes.adoc", ":b: 2\n:c: 3\n"),
        ("more.adoc", ":d: 4\n"),
    ];
    for mode in MODES {
        let options = with_includes(mode, &files)?;

        // The header ends in an included file.
        for (source, line) in [
            (":a: 1\ninclude::attributes.adoc[]\n\nPara\n", 0),
            ("= Title\n:a: 1\ninclude::attributes.adoc[]\n\nPara\n", 1),
        ] {
            assert_eq!(span_in_includes(source, &options)?, (false, true));
            let tokens = decoded_in(source, &options)?;
            assert!(
                has_token(&tokens, line, "a", "property"),
                "{mode:?}: {tokens:?}"
            );
            assert!(
                has_kind_on_line(&tokens, line + 1, "function"),
                "{mode:?}: {tokens:?}"
            );
        }

        // Entries between two includes, ending in the second.
        let source = ":a: 1\ninclude::attributes.adoc[]\n:x: 2\ninclude::more.adoc[]\n\nPara\n";
        assert_eq!(span_in_includes(source, &options)?, (false, true));
        let tokens = decoded_in(source, &options)?;
        assert!(
            has_token(&tokens, 0, "a", "property"),
            "{mode:?}: {tokens:?}"
        );
        assert!(
            has_token(&tokens, 2, "x", "property"),
            "{mode:?}: {tokens:?}"
        );

        // The header starts in an included file.
        let source = "include::attributes.adoc[]\n= Title\n:a: 1\n// note\n\nPara\n";
        assert_eq!(span_in_includes(source, &options)?, (true, false));
        let tokens = decoded_in(source, &options)?;
        assert!(
            has_token(&tokens, 1, "Title", "namespace"),
            "{mode:?}: {tokens:?}"
        );
        assert!(
            has_token(&tokens, 2, "a", "property"),
            "{mode:?}: {tokens:?}"
        );
        assert!(
            has_token(&tokens, 3, "// note", "comment"),
            "{mode:?}: {tokens:?}"
        );
    }
    Ok(())
}
