use super::{AttributeQuote, BlockAttributeMode, expand_attribute_list, scan_block_attribute_list};
use crate::{AttributeValue, CompatibilityMode, DocumentAttributes};

type Error = Box<dyn std::error::Error>;

fn attributes(values: &[(&str, &str)]) -> Result<DocumentAttributes<'static>, Error> {
    let mut attributes = DocumentAttributes::default();
    for (name, value) in values {
        attributes.insert(
            (*name).to_string().into(),
            AttributeValue::String((*value).to_string().into()),
        )?;
    }
    Ok(attributes)
}

#[test]
fn expanded_separators_keep_their_reference_span() -> Result<(), Error> {
    let attributes = attributes(&[("entries", r#"data="café",other="λ""#)])?;
    let source = "role=before,{entries},last=end";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let expanded = expand_attribute_list(source, &attributes, mode);
        let values = scan_block_attribute_list(&expanded.text, mode, BlockAttributeMode::Block);
        for name in ["data", "other"] {
            let value = values
                .iter()
                .find(|value| value.name.as_deref() == Some(name))
                .ok_or("missing value")?;
            let span = value.source_range(&expanded.source_map, 0..value.value.len())?;
            assert_eq!(source.get(span), Some("{entries}"));
        }
        let last = values.last().ok_or("missing last value")?;
        assert_eq!(
            source.get(last.source_range(&expanded.source_map, 0..last.value.len())?),
            Some("end")
        );
    }
    Ok(())
}

#[test]
fn quote_removal_and_expansion_compose_without_losing_escape_bytes() -> Result<(), Error> {
    let attributes = attributes(&[("word", "café")])?;
    let source = r#"data="α\"{word}\"β""#;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let expanded = expand_attribute_list(source, &attributes, mode);
        let values = scan_block_attribute_list(&expanded.text, mode, BlockAttributeMode::Block);
        let [value] = values.as_slice() else {
            return Err("expected one value".into());
        };
        assert_eq!(value.value, "α\"café\"β");
        for (needle, authored) in [("α", "α"), ("\"", r#"\""#), ("café", "{word}"), ("β", "β")]
        {
            let start = value.value.find(needle).ok_or("missing decoded text")?;
            let span = value.source_range(&expanded.source_map, start..start + needle.len())?;
            assert_eq!(source.get(span), Some(authored));
        }
    }
    Ok(())
}

#[test]
fn inserted_references_are_not_expanded_again() -> Result<(), Error> {
    let attributes = attributes(&[("saved", "{next}"), ("next", "changed")])?;
    let source = r#"data="{saved}",other=\{next}"#;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let expanded = expand_attribute_list(source, &attributes, mode);
        let values = scan_block_attribute_list(&expanded.text, mode, BlockAttributeMode::Block);
        let [first, second] = values.as_slice() else {
            return Err("expected two values".into());
        };
        assert_eq!(first.value, "{next}");
        assert_eq!(second.value, "{next}");
        assert_eq!(
            source.get(first.source_range(&expanded.source_map, 0..first.value.len())?),
            Some("{saved}")
        );
        assert_eq!(
            source.get(second.source_range(&expanded.source_map, 0..second.value.len())?),
            Some(r"\{next}")
        );
    }
    Ok(())
}

#[test]
fn even_backslashes_can_end_a_draft_quoted_value() -> Result<(), Error> {
    let source = r#"data="α\\",role=after"#;
    let values =
        scan_block_attribute_list(source, CompatibilityMode::Draft, BlockAttributeMode::Block);
    let value = values.first().ok_or("missing value")?;
    assert_eq!(value.value, "α\\");
    assert_eq!(value.quote, AttributeQuote::Double);
    let values = scan_block_attribute_list(
        source,
        CompatibilityMode::Asciidoctor,
        BlockAttributeMode::Block,
    );
    let value = values.first().ok_or("missing value")?;
    assert_eq!(value.value, r#""α\\""#);
    assert_eq!(value.quote, AttributeQuote::Unquoted);
    Ok(())
}

#[test]
fn non_syntax_backslashes_stay_literal() -> Result<(), Error> {
    let attributes = attributes(&[])?;
    let source = r#"data="C:\path\file",other=\{not a reference}"#;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let expanded = expand_attribute_list(source, &attributes, mode);
        assert_eq!(expanded.text, source);
        let values = scan_block_attribute_list(&expanded.text, mode, BlockAttributeMode::Block);
        let first = values.first().ok_or("missing value")?;
        assert_eq!(first.value, r"C:\path\file");
    }
    Ok(())
}
