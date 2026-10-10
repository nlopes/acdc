//! Link attributes and cross-reference labels.

use crate::{
    ElementAttributes, InlineNode,
    grammar::{
        ParserState,
        helpers::BlockParsingMetadata,
        inline_preprocessor::{ProcessedKind, SourceMap},
        inline_processing::process_inlines_no_autolinks,
        inlines::{
            index_terms::IndexTermSegment,
            text::{RestoredRange, registration_source, restored_label_offset},
        },
        state::InlineRules,
    },
};
use std::{borrow::Cow, ops::Range};

/// The parts of `xref:target[...]` that decide what the link shows.
pub(super) struct XrefMacroText<'s> {
    /// The link text, empty for an automatic reference.
    pub(super) text: Cow<'s, str>,
    /// Where `text` starts within the brackets.
    pub(super) offset: usize,
    /// Maps unescaped label positions back to the original label.
    pub(super) source_map: SourceMap,
    /// Whether `text` is the whole bracket content as written.
    pub(super) as_written: bool,
    /// A `xrefstyle=` for this reference alone, overriding the document's.
    pub(super) xrefstyle: Option<String>,
    /// A `role=` for the link.
    pub(super) role: Option<String>,
}

/// Read `xref:target[...]`'s brackets the way Asciidoctor does.
///
/// Brackets holding an `=` are an attribute list: the first positional
/// attribute is the link text, `xrefstyle=` overrides the document's style
/// for this reference, so `xref:fig[xrefstyle=short]` prints `Figure 1`, and
/// `role=` is kept for the link. Any other brackets, and all brackets in
/// compat mode, are the link text as written, commas included.
///
/// Other named attributes are read and set aside rather than shown, so
/// `xref:fig[id=x]` still has automatic text, as in Asciidoctor.
pub(super) fn xref_macro_text(raw: &str, compat_mode: bool) -> XrefMacroText<'_> {
    if compat_mode || !raw.contains('=') {
        return XrefMacroText {
            text: Cow::Borrowed(raw),
            offset: 0,
            source_map: SourceMap::default(),
            as_written: true,
            xrefstyle: None,
            role: None,
        };
    }
    let mut parsed = XrefMacroText {
        text: Cow::Borrowed(""),
        offset: 0,
        source_map: SourceMap::default(),
        as_written: false,
        xrefstyle: None,
        role: None,
    };
    for (index, attribute) in crate::grammar::attributes::scan_attribute_list(raw, &[])
        .into_iter()
        .enumerate()
    {
        match attribute.name.as_deref() {
            Some("xrefstyle") => parsed.xrefstyle = Some(attribute.value),
            // A repeated attribute takes its last value, as in Asciidoctor.
            Some("role") => parsed.role = Some(attribute.value),
            // Positional attributes are numbered by their place in the list,
            // so text after a named attribute is not the link text:
            // `xref:fig[xrefstyle=short,Words]` is still automatic.
            None if index == 0 => {
                let slice = raw
                    .get(attribute.value_start..attribute.value_end)
                    .unwrap_or_default();
                // A quoted value with escaped quotes reads differently from
                // the source slice; only then does the text need its own copy.
                parsed.text = if slice == attribute.value {
                    Cow::Borrowed(slice)
                } else {
                    if let Some(quote @ ('\'' | '"')) = raw
                        .get(..attribute.value_start)
                        .and_then(|prefix| prefix.chars().next_back())
                    {
                        for (offset, _) in slice.match_indices(&format!("\\{quote}")) {
                            parsed.source_map.add_replacement(
                                offset,
                                offset + 2,
                                1,
                                ProcessedKind::Escape,
                            );
                        }
                    }
                    Cow::Owned(attribute.value)
                };
                parsed.offset = attribute.value_start;
            }
            Some(_) | None => {}
        }
    }
    parsed
}

#[derive(Debug)]
pub(super) struct LinkContent<'a> {
    pub(super) raw: &'a str,
    pub(super) protected: Vec<Range<usize>>,
}

impl LinkContent<'_> {
    // Keep nested macro delimiters protected when late attributes change offsets.
    fn registered_protected_ranges(&self, restored: &[RestoredRange]) -> Cow<'_, [Range<usize>]> {
        if restored.is_empty() {
            Cow::Borrowed(self.protected.as_slice())
        } else {
            let reversed: Vec<_> = restored
                .iter()
                .map(|(a, b)| (b.clone(), a.clone()))
                .collect();
            Cow::Owned(
                self.protected
                    .iter()
                    .map(|range| {
                        restored_label_offset(range.start, &reversed, false)
                            ..restored_label_offset(range.end, &reversed, false)
                    })
                    .collect(),
            )
        }
    }
}

#[derive(Default)]
pub(super) struct ProcessedLinkContent<'a> {
    pub(super) text: Vec<InlineNode<'a>>,
    pub(super) attributes: ElementAttributes<'a>,
    pub(super) subject: Option<&'a str>,
    pub(super) body: Option<&'a str>,
}

pub(super) fn process_link_content<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockParsingMetadata<'_>,
    start: usize,
    end: usize,
    content: &LinkContent<'a>,
    mailto: bool,
) -> Result<ProcessedLinkContent<'a>, crate::Error> {
    // URI arguments freeze when macros run; later attributes may still change
    // the visible label, but must not introduce query values or separators.
    let (raw, restored) = if mailto {
        registration_source(
            state,
            IndexTermSegment {
                text: content.raw,
                start,
            },
        )
    } else {
        (content.raw, Vec::new())
    };
    let protected = content.registered_protected_ranges(&restored);
    let mut text = content.raw;
    let mut offset = 0;
    let mut quote = None;
    let mut result = ProcessedLinkContent::default();
    // Mailto uses commas to introduce subject/body slots. Other links use '=';
    // without that trigger (or in compat mode), quotes and commas are label text.
    if !state.document_attributes.contains_key("compat-mode")
        && raw.contains(if mailto { ',' } else { '=' })
    {
        text = "";
        for (index, attribute) in crate::grammar::attributes::scan_attribute_list(raw, &protected)
            .into_iter()
            .enumerate()
        {
            let attribute_quote = raw[..attribute.value_start]
                .chars()
                .next_back()
                .filter(|character| matches!(character, '\'' | '"'));
            if let Some(name) = attribute.name {
                let name = match name.as_str() {
                    "roles" => Cow::Borrowed("role"),
                    "opts" => Cow::Borrowed("options"),
                    _ => Cow::Owned(name),
                };
                // Named presentation attributes still see late substitutions.
                let value = if restored.is_empty() {
                    attribute.value
                } else {
                    let start = restored_label_offset(attribute.value_start, &restored, false);
                    let end = restored_label_offset(attribute.value_end, &restored, false);
                    let value = &content.raw[start..end];
                    attribute_quote.map_or_else(
                        || value.to_string(),
                        |quote| crate::grammar::attributes::unescape_attribute_quote(value, quote),
                    )
                };
                result.attributes.set(name, value.into());
            } else if index == 0 {
                quote = attribute_quote;
                offset = restored_label_offset(attribute.value_start, &restored, false);
                let end = restored_label_offset(attribute.value_end, &restored, false);
                text = &content.raw[offset..end];
            } else if mailto && matches!(index, 1 | 2) {
                let value = state.intern_str(&attribute.value.replace("\\]", "]"));
                if index == 1 {
                    result.subject = Some(value);
                } else {
                    result.body = Some(value);
                }
            }
        }
    }
    if let Some(label) = text.strip_suffix('^') {
        text = label;
        result.attributes.insert("window".into(), "_blank".into());
    }
    if text.is_empty() {
        return Ok(result);
    }

    // Keep source slices intact until the label parser consumes each escape.
    // Reset the quote mode for nested links, then restore the enclosing mode on errors too.
    let rules = state.inline_ctx.rules;
    state.inline_ctx.rules.insert(InlineRules::BRACKET_LABEL);
    state
        .inline_ctx
        .rules
        .set(InlineRules::DOUBLE_QUOTED_LINK, quote == Some('"'));
    state
        .inline_ctx
        .rules
        .set(InlineRules::SINGLE_QUOTED_LINK, quote == Some('\''));
    let parsed = process_inlines_no_autolinks(
        state,
        metadata,
        start + offset,
        end,
        state.inline_ctx.offset,
        text,
    );
    state.inline_ctx.rules = rules;
    result.text = parsed?;
    Ok(result)
}
