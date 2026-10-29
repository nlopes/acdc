//! Link attributes and cross-reference labels.

use crate::{
    ContentAttribute, CrossReference, ElementId, InlineMetadata, InlineNode,
    grammar::{
        ParserState,
        attributes::ScannedAttribute,
        helpers::BlockParsingMetadata,
        inline_preprocessor::{ProcessedKind, SourceMap},
        inline_processing::process_inlines_no_autolinks,
        inlines::{
            index_terms::IndexTermSegment,
            text::{RestoredRange, registration_source, restored_label_offset},
        },
        state::InlineRules,
    },
    model::{MacroAttributes, XrefSourceSyntax},
};
use std::{borrow::Cow, ops::Range};

pub(in crate::grammar) fn force_local_file_xref(xref: &mut CrossReference<'_>) {
    if let Some(fragment) = file_xref_fragment(xref.target) {
        xref.target = fragment;
        xref.target_is_local = true;
        xref.source_syntax = XrefSourceSyntax::LocalFragment;
    }
}

fn file_xref_fragment(target: &str) -> Option<&str> {
    // Restore protected text before checking whether the prefix is a URL.
    if target.contains("���") {
        return None;
    }
    let (path, fragment) = target.split_once('#')?;
    let has_scheme = path.split_once(':').is_some_and(|(scheme, _)| {
        let Some((first, rest)) = scheme.as_bytes().split_first() else {
            return false;
        };
        // A single letter is a Windows drive prefix, as it is for includes.
        !rest.is_empty()
            && first.is_ascii_alphabetic()
            && rest
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    });
    (!fragment.is_empty() && !path.starts_with("//") && !has_scheme).then_some(fragment)
}

/// The label and controls in `xref:target[...]`.
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
    attributes: Vec<ScannedAttribute>,
}

/// Read `xref:target[...]`'s brackets the way Asciidoctor does.
///
/// Brackets holding an `=` are an attribute list: the first positional
/// attribute is the link text, `xrefstyle=` overrides the document's style
/// for this reference, so `xref:fig[xrefstyle=short]` prints `Figure 1`, and
/// named controls are retained. Any other brackets, and all brackets in
/// compat mode, are the link text as written, commas included.
///
/// Named attributes do not supply display text, so
/// `xref:fig[id=x]` still has automatic text, as in Asciidoctor.
pub(super) fn xref_macro_text(raw: &str, compat_mode: bool) -> XrefMacroText<'_> {
    if compat_mode || !raw.contains('=') {
        return XrefMacroText {
            text: Cow::Borrowed(raw),
            offset: 0,
            source_map: SourceMap::default(),
            as_written: true,
            xrefstyle: None,
            attributes: Vec::new(),
        };
    }
    let mut parsed = XrefMacroText {
        text: Cow::Borrowed(""),
        offset: 0,
        source_map: SourceMap::default(),
        as_written: false,
        xrefstyle: None,
        attributes: Vec::new(),
    };
    for (index, attribute) in crate::grammar::attributes::scan_attribute_list(raw, &[])
        .into_iter()
        .enumerate()
    {
        match attribute.name.as_deref() {
            Some("xrefstyle") => parsed.xrefstyle = Some(attribute.value),
            // A repeated attribute takes its last value, as in Asciidoctor.
            Some(_) => parsed.attributes.push(attribute),
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
            None => {}
        }
    }
    parsed
}

impl XrefMacroText<'_> {
    pub(super) fn take_metadata<'a>(
        &mut self,
        state: &ParserState<'a>,
        offset: usize,
    ) -> (InlineMetadata<'a>, MacroAttributes<'a>) {
        let mut metadata = InlineMetadata::default();
        let mut attributes = MacroAttributes::default();
        for attribute in std::mem::take(&mut self.attributes) {
            let Some(name) = attribute.name else {
                continue;
            };
            let name = match name.as_str() {
                "roles" => Cow::Borrowed("role"),
                "opts" => Cow::Borrowed("options"),
                _ => Cow::Owned(name),
            };
            insert_named_metadata(
                state,
                &mut metadata,
                &mut attributes,
                name,
                attribute.value,
                offset + attribute.value_start..offset + attribute.value_end,
            );
        }
        (metadata, attributes)
    }
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
    pub(super) tooltip: Option<ContentAttribute<'a>>,
    pub(super) metadata: InlineMetadata<'a>,
    pub(super) macro_attributes: MacroAttributes<'a>,
    pub(super) subject: Option<&'a str>,
    pub(super) body: Option<&'a str>,
}

fn insert_named_metadata<'a>(
    state: &ParserState<'a>,
    metadata: &mut InlineMetadata<'a>,
    attributes: &mut MacroAttributes<'a>,
    name: Cow<'a, str>,
    value: String,
    range: Range<usize>,
) {
    match name.as_ref() {
        "id" => {
            metadata.id = Some(ElementId {
                id: state.intern_str(&value),
                location: state
                    .create_location(range.start, range.end.saturating_sub(1).max(range.start)),
            });
        }
        "role" => {
            metadata.roles = value
                .split_whitespace()
                .map(|role| state.intern_str(role))
                .collect();
        }
        "options" => {
            attributes.options =
                crate::grammar::helpers::parse_comma_separated_values(state, &value);
        }
        _ => attributes.attributes.set(name, value.into()),
    }
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
    let mut tooltip = None;
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
                if name == "title" {
                    let from = restored_label_offset(attribute.value_start, &restored, false);
                    let to = restored_label_offset(attribute.value_end, &restored, false);
                    tooltip = Some((from, to, attribute.quote));
                    continue;
                }
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
                let from = start + restored_label_offset(attribute.value_start, &restored, false);
                let to = start + restored_label_offset(attribute.value_end, &restored, false);
                insert_named_metadata(
                    state,
                    &mut result.metadata,
                    &mut result.macro_attributes,
                    name,
                    value,
                    from..to,
                );
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
        result
            .macro_attributes
            .attributes
            .insert("window".into(), "_blank".into());
    }
    if !text.is_empty() {
        result.text = parse_link_label(state, metadata, text, start + offset, end, quote)?;
    }
    if let Some((from, to, quote)) = tooltip {
        result.tooltip = Some(super::attributes::parse_content(
            state,
            &content.raw[from..to],
            start + from,
            quote,
        )?);
    }
    Ok(result)
}

fn parse_link_label<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockParsingMetadata<'_>,
    text: &'a str,
    start: usize,
    end: usize,
    quote: Option<char>,
) -> Result<Vec<crate::InlineNode<'a>>, crate::Error> {
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
    let parsed =
        process_inlines_no_autolinks(state, metadata, start, end, state.inline_ctx.offset, text);
    state.inline_ctx.rules = rules;
    parsed
}
