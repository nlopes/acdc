use acdc_parser::{
    DocumentAttributeValue, SourceLocation,
    ast::{BlockKind, DocumentAttributeRef, DocumentRef, StructuralRef},
};

use crate::LintId;

use super::{LintEmitter, SourceLine, line_range_for_location, source_lines_for_range};

pub(crate) fn lint_attribute_url_prefix(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
    lines: &[SourceLine<'_>],
) {
    lint_header_attribute_url_prefix(emitter, document, lines);
    for node in document.descendants() {
        if let StructuralRef::Block(block) = node
            && let BlockKind::DocumentAttribute(attribute) = block.kind()
        {
            lint_document_attribute_url_prefix(emitter, attribute);
        }
    }
}

fn lint_header_attribute_url_prefix(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
    lines: &[SourceLine<'_>],
) {
    let Some(header) = document.header() else {
        return;
    };
    let Some(range) = line_range_for_location(header.location()) else {
        return;
    };
    for line in source_lines_for_range(lines, range) {
        let Some(attribute) = parse_attribute_line(line.text.trim()) else {
            continue;
        };
        lint_url_attribute(
            emitter,
            attribute.name,
            attribute.value,
            emitter.point_location(line.number, 1),
        );
    }
}

fn lint_document_attribute_url_prefix(
    emitter: &mut LintEmitter<'_>,
    attribute: DocumentAttributeRef<'_>,
) {
    let Some(value) = attribute
        .assignment()
        .value()
        .and_then(DocumentAttributeValue::as_str)
    else {
        return;
    };
    lint_url_attribute(
        emitter,
        attribute.name(),
        value,
        emitter.source_location(attribute.location()),
    );
}

fn lint_url_attribute(
    emitter: &mut LintEmitter<'_>,
    name: &str,
    value: &str,
    location: SourceLocation,
) {
    let value = value.trim();
    if !is_url_value(value) || name.starts_with("url-") || name.starts_with("uri-") {
        return;
    }
    emitter.emit(
        LintId::AttributeUrlPrefix,
        format!("URL-valued attribute `{name}` should use a url- or uri- prefix"),
        Some(format!("rename `{name}` to `url-{name}`")),
        Some(location),
    );
}

struct AttributeLine<'a> {
    name: &'a str,
    value: &'a str,
}

fn parse_attribute_line(trimmed: &str) -> Option<AttributeLine<'_>> {
    let rest = trimmed.strip_prefix(':')?;
    let (name, value) = rest.split_once(':')?;
    if name.is_empty() || name.starts_with('!') || name.ends_with('!') {
        return None;
    }
    let value = value.strip_prefix(' ').unwrap_or(value).trim();
    (!value.is_empty()).then_some(AttributeLine { name, value })
}

fn is_url_value(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
        && scheme
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        && (rest.starts_with("//") || matches!(scheme, "mailto" | "urn"))
}

#[cfg(test)]
mod tests {
    use crate::{Error, LintId};

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn literal_quotes_do_not_make_document_text_a_url() -> Result<(), Error> {
        for value in [
            "'https://example.com'",
            "\"https://example.com\"",
            "'https://example.com",
        ] {
            for separator in ["\n", "\n\n"] {
                let source = format!("= Title{separator}:docs: {value}\n\nContent.\n");
                let report = report_for(&source)?;
                assert!(!has_lint(&report, LintId::AttributeUrlPrefix), "{source}");
            }
        }
        Ok(())
    }

    #[test]
    fn attribute_url_prefix_flags_url_values_without_prefix() -> Result<(), Error> {
        let report = report_for("= Title\n\n:docs: https://example.com\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::AttributeUrlPrefix));
        Ok(())
    }

    #[test]
    fn attribute_url_prefix_flags_header_attributes() -> Result<(), Error> {
        let report = report_for("= Title\n:docs: https://example.com\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::AttributeUrlPrefix));
        Ok(())
    }

    #[test]
    fn attribute_url_prefix_allows_url_prefixed_names() -> Result<(), Error> {
        let report = report_for("= Title\n\n:url-docs: https://example.com\n\nContent.\n")?;

        assert!(!has_lint(&report, LintId::AttributeUrlPrefix));
        Ok(())
    }

    #[test]
    fn attribute_url_prefix_ignores_attributes_inside_listing() -> Result<(), Error> {
        let report = report_for("= Title\n\n----\n:docs: https://example.com\n----\n")?;

        assert!(!has_lint(&report, LintId::AttributeUrlPrefix));
        Ok(())
    }
}
