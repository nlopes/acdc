//! End-to-end behaviour of the `list-of::` pass, asserted on the rewritten AST.
//!
//! The pass produces cross-references, not rendered text, so the assertions
//! read the generated nodes rather than any backend's output. `summary`
//! flattens a list into one line per entry — `image-1 | Figure 1 | The logo` —
//! which is close enough to what a reader sees to be worth comparing against.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use acdc_converters_core::inlines_to_string;
use acdc_parser::{Block, CrossReference, InlineMacro, InlineNode, ParseResult, XrefStyle};

/// Parse `input` and run the list pass over it.
fn process(input: &str) -> (ParseResult, Vec<String>) {
    let options = acdc_parser::Options::default();
    let mut parsed = acdc_parser::parse(input, &options).expect("input parses");
    let processor = acdc_lists::Processor::new();
    let mut warnings = Vec::new();
    parsed.with_document_mut(|document, arena| {
        processor.process(document, arena, &mut warnings);
    });
    let messages = warnings
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    (parsed, messages)
}

/// Every block in the document, flattened in document order.
fn flatten<'b, 'a>(blocks: &'b [Block<'a>], out: &mut Vec<&'b Block<'a>>) {
    for block in blocks {
        out.push(block);
        if let Block::Section(section) = block {
            flatten(&section.content, out);
        }
    }
}

/// The section titles the document still has, in order.
fn section_titles(parsed: &ParseResult) -> Vec<String> {
    let mut blocks = Vec::new();
    flatten(&parsed.document().blocks, &mut blocks);
    blocks
        .iter()
        .filter_map(|block| {
            if let Block::Section(section) = block {
                Some(inlines_to_string(&section.title))
            } else {
                None
            }
        })
        .collect()
}

/// One line per generated entry: target, xref style, then the trailing text.
fn summary(parsed: &ParseResult, section_title: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    flatten(&parsed.document().blocks, &mut blocks);
    let section = blocks
        .iter()
        .find_map(|block| {
            let Block::Section(section) = block else {
                return None;
            };
            (inlines_to_string(&section.title) == section_title).then_some(section)
        })
        .unwrap_or_else(|| panic!("no section titled {section_title}"));

    let paragraph = section
        .content
        .iter()
        .find_map(|block| {
            if let Block::Paragraph(paragraph) = block {
                Some(paragraph)
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("section {section_title} holds no paragraph"));

    let mut lines = Vec::new();
    let mut current = String::new();
    for node in &paragraph.content {
        if matches!(node, InlineNode::LineBreak(_)) {
            lines.push(std::mem::take(&mut current));
        } else if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = node {
            current.push_str(&describe(xref));
        } else {
            current.push_str(&inlines_to_string(std::slice::from_ref(node)));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn describe(xref: &CrossReference<'_>) -> String {
    let style = if xref.xrefstyle == XrefStyle::Short {
        "short"
    } else if xref.xrefstyle == XrefStyle::Full {
        "full"
    } else {
        "basic"
    };
    format!("{}[{style}]", xref.target)
}

#[test]
fn lists_captioned_images_with_generated_ids() {
    let input = "= T\n\n\
        .The wonderful linux logo\n\
        image::tux.svg[]\n\n\
        .Another image\n\
        image::svg.png[]\n\n\
        == List of figures\n\
        list-of::image[]\n";
    let (parsed, warnings) = process(input);

    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        summary(&parsed, "List of figures"),
        [
            "image-1[short] The wonderful linux logo",
            "image-2[short] Another image",
        ]
    );
    // The generated ids have to be in the catalog, or the references would
    // render as `[image-1]`.
    assert!(parsed.document().references.contains_key("image-1"));
    assert!(parsed.document().references.contains_key("image-2"));
}

#[test]
fn keeps_an_id_the_document_already_gave_the_element() {
    let input = "= T\n\n\
        [#tux]\n\
        .The logo\n\
        image::tux.svg[]\n\n\
        == List of figures\n\
        list-of::image[]\n";
    let (parsed, _) = process(input);

    assert_eq!(summary(&parsed, "List of figures"), ["tux[short] The logo"]);
    assert!(!parsed.document().references.contains_key("image-1"));
}

#[test]
fn does_not_reuse_an_id_the_document_already_uses() {
    let input = "= T\n\n\
        [#image-1]\n\
        .Taken\n\
        image::a.svg[]\n\n\
        .Needs an id\n\
        image::b.svg[]\n\n\
        == List of figures\n\
        list-of::image[]\n";
    let (parsed, _) = process(input);

    assert_eq!(
        summary(&parsed, "List of figures"),
        ["image-1[short] Taken", "image-2[short] Needs an id"]
    );
}

#[test]
fn lists_each_element_kind_under_its_own_caption() {
    let input = "= T\n\
        :listing-caption: Code\n\n\
        .A table\n\
        |===\n|A |B\n|===\n\n\
        .Some code\n\
        ----\nputs 1\n----\n\n\
        == List of tables\n\
        list-of::table[]\n\n\
        == List of code snippets\n\
        list-of::listing[]\n";
    let (parsed, _) = process(input);

    assert_eq!(
        summary(&parsed, "List of tables"),
        ["table-1[short] A table"]
    );
    assert_eq!(
        summary(&parsed, "List of code snippets"),
        ["listing-1[short] Some code"]
    );
}

#[test]
fn finds_elements_inside_a_table_cell() {
    let input = "= T\n\n\
        |===\n\
        a|.Nested\nimage::a.svg[]\n\
        |===\n\n\
        == List of figures\n\
        list-of::image[]\n";
    let (parsed, _) = process(input);

    assert_eq!(
        summary(&parsed, "List of figures"),
        ["image-1[short] Nested"]
    );
}

#[test]
fn hide_empty_section_removes_the_section_around_an_empty_list() {
    let input = "= T\n\n\
        .An image\n\
        image::a.svg[]\n\n\
        == List of figures\n\
        list-of::image[hide_empty_section=true]\n\n\
        == List of tables\n\
        list-of::table[hide_empty_section=true]\n";
    let (parsed, _) = process(input);

    assert_eq!(section_titles(&parsed), ["List of figures"]);
}

#[test]
fn an_empty_list_without_the_flag_leaves_its_section_behind() {
    let input = "= T\n\n\
        == List of tables\n\
        list-of::table[]\n";
    let (parsed, _) = process(input);

    assert_eq!(section_titles(&parsed), ["List of tables"]);
    let mut blocks = Vec::new();
    flatten(&parsed.document().blocks, &mut blocks);
    // The call itself is gone, so nothing is left to render.
    assert!(
        !blocks
            .iter()
            .any(|block| matches!(block, Block::Paragraph(_)))
    );
}

#[test]
fn reports_an_element_name_it_does_not_know() {
    let input = "= T\n\n== List\nlist-of::figure[]\n";
    let (parsed, warnings) = process(input);

    let [warning] = warnings.as_slice() else {
        panic!("expected exactly one warning: {warnings:?}");
    };
    assert!(
        warning.contains("`list-of::figure[]` names an unknown element"),
        "unexpected warning: {warning}"
    );
    // The call is left as written, so the mistake stays visible in the output.
    assert_eq!(summary(&parsed, "List"), ["list-of::figure[]"]);
}

#[test]
fn leaves_an_ordinary_paragraph_alone() {
    let input = "= T\n\n== List\nSee list-of::image[] for details.\n";
    let (parsed, warnings) = process(input);

    assert!(warnings.is_empty());
    assert_eq!(
        summary(&parsed, "List"),
        ["See list-of::image[] for details."]
    );
}
