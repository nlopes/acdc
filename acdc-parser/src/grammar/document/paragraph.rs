//! Interpret paragraph styles after their source boundaries have been matched.

use crate::{
    CompatibilityMode, Error, InlineNode,
    grammar::{
        ParserState,
        helpers::BlockParsingMetadata,
        inline_preprocessor::{ProcessedKind, SourceMap},
        inline_processing::process_mapped_inlines,
    },
};

pub(super) fn indented_style(mode: CompatibilityMode, style: Option<&str>) -> Option<&str> {
    match style {
        Some("paragraph") if mode == CompatibilityMode::Draft => Some("normal"),
        Some(
            "normal" | "source" | "listing" | "literal" | "verse" | "quote" | "sidebar" | "example"
            | "abstract" | "partintro" | "pass" | "comment" | "NOTE" | "TIP" | "IMPORTANT"
            | "CAUTION" | "WARNING",
        ) => style,
        _ => None,
    }
}

pub(super) fn process_normal_indented<'a>(
    state: &ParserState<'a>,
    metadata: &BlockParsingMetadata<'a>,
    content: &'a str,
    absolute_start: usize,
) -> Result<Vec<InlineNode<'a>>, Error> {
    let indent = common_indent(content);
    let mut text = String::with_capacity(content.len());
    let mut map = SourceMap::default();
    let mut start = 0;
    for line in content.split_inclusive('\n') {
        let removed = line
            .bytes()
            .take(indent)
            .take_while(|byte| *byte == b' ' || *byte == b'\t')
            .count();
        if removed != 0 {
            map.add_replacement(start, start + removed, 0, ProcessedKind::Indentation);
        }
        text.push_str(line.get(removed..).unwrap_or_default());
        start += line.len();
    }
    let map = map.with_origin(SourceMap::default(), absolute_start);
    process_mapped_inlines(
        state,
        state.intern_str(&text),
        &map,
        metadata,
        &state.document_attributes,
    )
}

pub(super) fn common_indent(content: &str) -> usize {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.bytes()
                .take_while(|byte| *byte == b' ' || *byte == b'\t')
                .count()
        })
        .min()
        .unwrap_or(0)
}
