//! Manpage attributes derived from the document header and name section.

use crate::{
    DocumentAttributes, Header, InlineNode,
    document_attribute::AttributeDeclaration,
    error::{Error, SourceLocation},
    grammar::{ParserState, document::doctype::is_manpage_doctype},
    model::{strip_quotes, substitute, substitution::HEADER},
};
use std::rc::Rc;

pub(super) struct ManpageNameSection<'input> {
    pub(super) title: &'input str,
    pub(super) attributes: NameSectionAttributes,
    pub(super) metadata_attributes: Vec<AttributeDeclaration<'input>>,
}

pub(super) fn prepare_manpage_name_attributes<'input>(
    state: &mut ParserState<'input>,
    section: Option<ManpageNameSection<'input>>,
) {
    if !is_manpage_doctype(&state.document_attributes) {
        return;
    }

    if let (Some(name), Some(purpose)) = (
        state.document_attributes.text("manname").map(strip_quotes),
        state
            .document_attributes
            .text("manpurpose")
            .map(strip_quotes),
    ) {
        let name = state.intern_str(name);
        let purpose = state.intern_str(purpose);
        set_manpage_name_attributes(state, name, Some(purpose), Some("Name"));
        return;
    }

    if let Some(section) = section {
        let mut attributes = DocumentAttributes::clone(&state.document_attributes);
        for AttributeDeclaration { name, value } in section.metadata_attributes {
            let value = value.resolve(&attributes);
            let _ = attributes.assign_document_value(name.into(), value, false, false, None);
        }

        let name = substitute(&section.attributes.name, HEADER, &attributes);
        let name = name.split(',').next().unwrap_or_default().trim();
        let name = state.intern_str(name);
        let purpose = substitute(&section.attributes.purpose, HEADER, &attributes);
        let purpose = state.intern_str(&purpose);
        let title = substitute(section.title, HEADER, &attributes);
        let title = state.intern_str(&title);
        set_manpage_name_attributes(state, name, Some(purpose), Some(title));
        return;
    }

    let fallback = state
        .document_attributes
        .text("docname")
        .map_or("command", strip_quotes);
    let fallback = state.intern_str(fallback);
    set_manpage_name_attributes(state, fallback, None, None);
}

fn set_manpage_name_attributes<'input>(
    state: &mut ParserState<'input>,
    name: &'input str,
    purpose: Option<&'input str>,
    title: Option<&'input str>,
) {
    let attributes = Rc::make_mut(&mut state.document_attributes);
    attributes.set_text("manname".into(), name.into());
    if let Some(purpose) = purpose {
        attributes.set_text("manpurpose".into(), purpose.into());
    }
    if let Some(title) = title {
        attributes.insert_text("manname-title".into(), title.into());
    }
    if attributes.text("backend").map(strip_quotes) == Some("manpage") {
        attributes.set_text("docname".into(), name.into());
    }
}

/// Parsed manpage title components.
#[derive(Debug, Clone)]
struct ManpageTitle {
    /// The program/command name (e.g., "git-commit").
    name: String,
    /// The volume number (e.g., "1", "3p", "8").
    volume: String,
}

/// Parse `name(volume)`. The volume must be one digit with an optional letter.
/// Return `None` if the title does not have this format.
fn parse_manpage_title(title: &str) -> Option<ManpageTitle> {
    let title = title.trim();
    if !title.ends_with(')') {
        return None;
    }

    let open_paren = title.rfind('(')?;
    if open_paren == 0 {
        return None; // No name before the paren
    }

    let name = title[..open_paren].trim();
    if name.is_empty() {
        return None;
    }

    let volume = title[open_paren + 1..title.len() - 1].trim();

    match volume.chars().collect::<Vec<char>>().as_slice() {
        [first] if first.is_ascii_digit() => {} // valid,
        [first, second] if first.is_ascii_digit() && second.is_ascii_alphabetic() => {}
        _ => {
            tracing::warn!("invalid manpage volume format in title");
            return None;
        }
    }

    Some(ManpageTitle {
        name: name.into(),
        volume: volume.into(),
    })
}

/// Extract plain text from inline nodes (for title parsing).
fn extract_plain_text(nodes: &[InlineNode]) -> String {
    let mut result = String::new();
    for node in nodes {
        match node {
            InlineNode::PlainText(text) => result.push_str(text.content),
            InlineNode::RawText(text) => result.push_str(text.content),
            InlineNode::VerbatimText(text) => result.push_str(text.content),
            InlineNode::BoldText(bold) => result.push_str(&extract_plain_text(&bold.content)),
            InlineNode::ItalicText(italic) => result.push_str(&extract_plain_text(&italic.content)),
            InlineNode::MonospaceText(mono) => result.push_str(&extract_plain_text(&mono.content)),
            InlineNode::HighlightText(highlight) => {
                result.push_str(&extract_plain_text(&highlight.content));
            }
            InlineNode::SubscriptText(sub) => result.push_str(&extract_plain_text(&sub.content)),
            InlineNode::SuperscriptText(sup) => result.push_str(&extract_plain_text(&sup.content)),
            InlineNode::CurvedQuotationText(quoted) => {
                result.push_str(&extract_plain_text(&quoted.content));
            }
            InlineNode::CurvedApostropheText(quoted) => {
                result.push_str(&extract_plain_text(&quoted.content));
            }
            // These nodes don't contribute plain text
            InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::CalloutRef(_)
            | InlineNode::Macro(_) => {}
        }
    }
    result
}

/// Make a lowercase `mantitle` from a filename or nonconforming title.
/// Replace characters other than letters, digits, `-`, and `_` with hyphens.
/// Collapse repeated hyphens and remove them from the ends.
fn sanitize_mantitle(name: &str) -> String {
    let sanitized: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in sanitized.chars() {
        if c == '-' {
            if !prev_hyphen && !result.is_empty() {
                result.push(c);
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    result.trim_end_matches('-').into()
}

/// Set `mantitle` and `manvolnum` after the header, before parsing body blocks.
/// Preserve caller values. Return `false` when there is no header.
///
/// Strict mode rejects titles outside the `name(volume)` format. Otherwise,
/// use the filename or cleaned title for `mantitle` and `1` for `manvolnum`.
pub(super) fn derive_manpage_header_attrs<'a>(
    header: Option<&Header<'a>>,
    attrs: &mut DocumentAttributes<'a>,
    strict: bool,
    source_file: Option<&std::path::Path>,
) -> Result<bool, Error> {
    let Some(header) = header else {
        return Ok(false);
    };

    let title_text = extract_plain_text(header.title.as_ref());

    if let Some(manpage_title) = parse_manpage_title(&title_text) {
        attrs.insert_text("mantitle".into(), manpage_title.name.to_lowercase().into());
        attrs.insert_text("manvolnum".into(), manpage_title.volume.into());

        tracing::debug!("derived manpage attributes from header");
    } else {
        if strict {
            return Err(Error::NonConformingManpageTitle(
                Box::new(SourceLocation {
                    file: source_file.map(std::path::Path::to_path_buf),
                    location: header.location.clone(),
                }),
                format!("title '{title_text}' does not match 'name(volume)' format"),
            ));
        }

        // Use fallbacks (matching asciidoctor behavior):
        // - mantitle: filename without extension (or sanitized title if no file)
        // - manvolnum: "1"
        let fallback_name = source_file
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or(&title_text);

        let sanitized = sanitize_mantitle(fallback_name);

        tracing::warn!(
            "doctype=manpage but title doesn't match name(volume) format; using filename as fallback"
        );

        attrs.insert_text("mantitle".into(), sanitized.into());
        attrs.insert_text("manvolnum".into(), "1".into());

        tracing::debug!("using fallback manpage attributes for non-conforming title");
    }

    if attrs.text("backend").map(crate::strip_quotes) == Some("manpage")
        && let Some(manvolnum) = attrs.text("manvolnum").map(crate::strip_quotes)
    {
        attrs.set_text("outfilesuffix".into(), format!(".{manvolnum}").into());
    }

    Ok(true)
}

/// Values read from the required first section of a manpage document.
#[derive(Debug, PartialEq)]
pub(super) struct NameSectionAttributes {
    name: String,
    purpose: String,
}

/// Derive manpage name metadata from the paragraph lines found by the grammar.
pub(super) fn derive_name_section_attrs<'a>(
    lines: impl IntoIterator<Item = Option<&'a str>>,
) -> Option<NameSectionAttributes> {
    let content = lines
        .into_iter()
        .flatten()
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join(" ");
    let (name, purpose) = split_name_purpose(&content)?;

    Some(NameSectionAttributes {
        name: name.to_string(),
        purpose: purpose.to_string(),
    })
}

fn split_name_purpose(content: &str) -> Option<(&str, &str)> {
    for (index, character) in content.char_indices() {
        if character != '-' {
            continue;
        }
        let name = content.get(..index)?.trim_end();
        let purpose = content.get(index + 1..)?.trim_start();
        if !name.is_empty()
            && !purpose.is_empty()
            && content.get(..index)?.ends_with(' ')
            && content.get(index + 1..)?.starts_with(' ')
        {
            return Some((name, purpose));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn test_parse_manpage_title_simple() {
        let title = parse_manpage_title("git(1)").expect("valid manpage title");
        assert_eq!(title.name, "git");
        assert_eq!(title.volume, "1");
    }

    #[test]
    fn test_parse_manpage_title_with_hyphen() {
        let title = parse_manpage_title("git-commit(1)").expect("valid manpage title");
        assert_eq!(title.name, "git-commit");
        assert_eq!(title.volume, "1");
    }

    #[test]
    fn test_parse_manpage_title_with_letter() {
        let title = parse_manpage_title("intro(3p)").expect("valid manpage title");
        assert_eq!(title.name, "intro");
        assert_eq!(title.volume, "3p");
    }

    #[test]
    fn test_parse_manpage_title_volume_5() {
        let title = parse_manpage_title("passwd(5)").expect("valid manpage title");
        assert_eq!(title.name, "passwd");
        assert_eq!(title.volume, "5");
    }

    #[test]
    fn test_parse_manpage_title_invalid() {
        assert!(parse_manpage_title("no-volume").is_none());
        assert!(parse_manpage_title("bad()").is_none());
        assert!(parse_manpage_title("wrong(abc)").is_none());
    }

    #[test]
    fn test_derive_name_section_attrs() {
        let attrs = derive_name_section_attrs([
            Some("myprogram, myalias - a test"),
            None,
            Some(" program"),
        ])
        .expect("valid NAME paragraph");
        assert_eq!(attrs.name, "myprogram, myalias");
        assert_eq!(attrs.purpose, "a test program");
    }

    #[test]
    fn test_derive_name_section_attrs_requires_a_spaced_separator() {
        assert!(derive_name_section_attrs([Some("cmd-test")]).is_none());
        assert!(derive_name_section_attrs([Some("cmd - ")]).is_none());
    }

    #[test]
    fn test_manpage_backend_uses_volume_as_output_suffix() -> Result<(), Error> {
        let mut attrs = DocumentAttributes::default();
        assert!(attrs.set("backend".into(), "manpage".into()).is_ok());
        assert!(attrs.set("doctype".into(), "manpage".into()).is_ok());
        assert!(attrs.set("outfilesuffix".into(), ".man".into()).is_ok());
        let options = crate::Options::with_attributes(attrs.into_inputs())?;

        let parsed = crate::parse("= cmd(7)\n\n== Name\n\ncmd - test\n", &options)?;

        assert_eq!(
            parsed.document().attributes.text("outfilesuffix"),
            Some(".7")
        );
        Ok(())
    }

    #[test]
    fn test_sanitize_mantitle_simple() {
        assert_eq!(sanitize_mantitle("My Document"), "my-document");
    }

    #[test]
    fn test_sanitize_mantitle_with_special_chars() {
        assert_eq!(
            sanitize_mantitle("Upcoming breaking changes"),
            "upcoming-breaking-changes"
        );
    }

    #[test]
    fn test_sanitize_mantitle_collapses_hyphens() {
        assert_eq!(sanitize_mantitle("foo  bar   baz"), "foo-bar-baz");
    }

    #[test]
    fn test_sanitize_mantitle_trims_hyphens() {
        assert_eq!(
            sanitize_mantitle("  Leading and trailing  "),
            "leading-and-trailing"
        );
    }

    #[test]
    fn test_sanitize_mantitle_preserves_underscores() {
        assert_eq!(sanitize_mantitle("my_document_name"), "my_document_name");
    }

    #[test]
    fn test_sanitize_mantitle_mixed_chars() {
        assert_eq!(
            sanitize_mantitle("Git 3.0: Breaking Changes!"),
            "git-3-0-breaking-changes"
        );
    }
}
