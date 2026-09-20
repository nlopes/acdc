//! End-to-end tests: a document goes in, a document with its citations
//! resolved comes out.
//!
//! The expectations were taken from running `asciidoctor -r
//! asciidoctor-bibtex` over the same sources, so a change that drifts from the
//! original extension shows up here.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::wildcard_enum_match_arm
)]

use std::{fmt::Write as _, fs, path::Path};

use acdc_bibtex::{Options, Processor};
use acdc_converters_core::Warning;
use acdc_parser::{Block, Document, InlineMacro, InlineNode};
use pretty_assertions::assert_eq;

/// The database every test below cites.
const DATABASE: &str = r"
@book{Lane12a,
    author = {P. Lane},
    title = {Book title},
    publisher = {Publisher},
    year = {2000}
}
@book{Lane12b,
    author = {K. Mane and D. Smith},
    title = {Book title},
    publisher = {Publisher},
    year = {2000}
}
@article{Anderson04,
    author = {J. R. Anderson and D. Bothell and M. D. Byrne and S. Douglass and C. Lebiere and Y. L. Qin},
    title = {An integrated theory of the mind},
    journal = {Psychological Review},
    volume = {111},
    number = {4},
    pages = {1036--1060},
    year = {2004}
}
";

/// Run the pass over a source and flatten what comes out.
///
/// The text is rendered the way `AsciiDoc` would write it — `_italic_`,
/// `<<key,text>>` for a link to an entry — so an expectation reads like the
/// document it describes.
fn convert(source: &str) -> (Vec<String>, Vec<Warning>) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("references.bib"), DATABASE).expect("write database");
    convert_in(directory.path(), source)
}

fn convert_in(base_dir: &Path, source: &str) -> (Vec<String>, Vec<Warning>) {
    convert_from(base_dir, base_dir, source)
}

/// Run the pass with the document and the command in different directories.
fn convert_from(base_dir: &Path, working_dir: &Path, source: &str) -> (Vec<String>, Vec<Warning>) {
    let mut parsed = acdc_parser::parse(source, &acdc_parser::Options::default()).expect("parse");
    let processor = Processor::new(
        Options::builder()
            .base_dir(base_dir)
            .working_dir(working_dir)
            .build(),
    );
    let mut warnings = Vec::new();
    parsed
        .with_document_mut(|document, arena| processor.process(document, arena, &mut warnings))
        .expect("resolve citations");
    let lines = parsed.with_document_mut(|document, _| flatten(document));
    (lines, warnings)
}

/// One line of text per block, with the formatting spelled out.
fn flatten(document: &Document<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    collect(&document.blocks, &mut lines);
    for footnote in &document.footnotes {
        lines.push(format!("footnote: {}", render(&footnote.content)));
    }

    lines
}

fn collect(blocks: &[Block<'_>], lines: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Section(section) => {
                lines.push(render(section.title.clone().into_inlines().as_slice()));
                collect(&section.content, lines);
            }
            Block::Paragraph(paragraph) => lines.push(render(&paragraph.content)),
            Block::UnorderedList(list) => {
                for item in &list.items {
                    lines.push(format!("* {}", render(&item.principal)));
                }
            }
            _ => {}
        }
    }
}

fn render(nodes: &[InlineNode<'_>]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::PlainText(text) => out.push_str(text.content),
            InlineNode::ItalicText(span) => {
                out.push('_');
                out.push_str(&render(&span.content));
                out.push('_');
            }
            InlineNode::HighlightText(span) => {
                let _ = write!(out, "[.{}]#", span.role.unwrap_or_default());
                out.push_str(&render(&span.content));
                out.push('#');
            }
            InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
                let _ = write!(out, "<<{},{}>>", xref.target, render(&xref.text));
            }
            InlineNode::Macro(InlineMacro::Link(link)) => {
                let _ = write!(out, "{}[{}]", link.target, render(&link.text));
            }
            InlineNode::InlineAnchor(anchor) => {
                let _ = write!(out, "[[{}]]", anchor.id);
            }
            InlineNode::Macro(InlineMacro::Footnote(footnote)) => {
                let _ = write!(out, "footnote:[{}]", render(&footnote.content));
            }
            InlineNode::RawText(raw) => out.push_str(raw.content),
            _ => {}
        }
    }
    out
}

fn document(body: &str) -> String {
    format!("= Paper\n:bibtex-file: references.bib\n\n{body}\n")
}

#[test]
fn a_numeric_citation_is_the_entry_position() {
    let (lines, warnings) = convert(&document(
        "Discussed in cite:[Lane12b] and cite:[Lane12a].\n\nbibliography::[]",
    ));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        lines[0],
        "Discussed in [.citation]#[<<Lane12b,1>>]# and [.citation]#[<<Lane12a,2>>]#."
    );
}

#[test]
fn the_bibliography_replaces_the_macro() {
    let (lines, _) = convert(&document("cite:[Anderson04]\n\nbibliography::[]"));
    assert_eq!(
        lines[1],
        "[[Anderson04]][1] J. R. Anderson, D. Bothell, M. D. Byrne, S. Douglass, C. Lebiere, \
         and Y. L. Qin, \u{201C}An integrated theory of the mind,\u{201D} \
         _Psychological Review_, vol. 111, no. 4, pp. 1036\u{2013}1060, 2004."
    );
}

#[test]
fn an_author_date_style_names_and_dates_the_work() {
    let source = "= Paper\n:bibtex-file: references.bib\n:bibtex-style: apa\n\n\
                  cite:[Lane12a] and citenp:[Lane12a].\n";
    let (lines, _) = convert(source);
    assert_eq!(
        lines[0],
        "[.citation]#(<<Lane12a,Lane, 2000>>)# and [.citation]#<<Lane12a,Lane (2000)>>#."
    );
}

#[test]
fn a_locator_follows_the_style() {
    let (lines, _) = convert(&document("cite:[Lane12a(59)] cite:[Lane12a(59-63)]"));
    assert_eq!(
        lines[0],
        "[.citation]#[<<Lane12a,1 p.\u{a0}59>>]# [.citation]#[<<Lane12a,1 pp.\u{a0}59-63>>]#"
    );
}

#[test]
fn pretext_sits_outside_a_numeric_citation() {
    let (lines, _) = convert(&document("cite:See[Lane12a]"));
    // The pretext goes inside the span but outside the brackets, so a
    // numeric citation reads "See [1]".
    assert_eq!(lines[0], "[.citation]#See [<<Lane12a,1>>]#");
}

#[test]
fn a_bibitem_renders_an_entry_where_it_stands() {
    let (lines, _) = convert(&document("* bibitem:[Lane12a]"));
    assert_eq!(lines[0], "* P. Lane, _Book title_. Publisher, 2000.");
}

#[test]
fn citations_in_a_footnote_are_resolved() {
    let (lines, _) = convert(&document("As noted footnote:[See cite:[Lane12a].]"));
    assert_eq!(
        lines.last().unwrap(),
        "footnote: See [.citation]#[<<Lane12a,1>>]#."
    );
}

#[test]
fn an_unknown_key_is_reported_and_left_as_written() {
    let (lines, warnings) = convert(&document("cite:[Nobody99]"));
    assert_eq!(lines[0], "[.citation]#[Nobody99]#");
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].message.contains("Nobody99"),
        "{:?}",
        warnings[0].message
    );
}

#[test]
fn an_unknown_key_can_be_made_fatal() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("references.bib"), DATABASE).expect("write database");
    let source = "= Paper\n:bibtex-file: references.bib\n:bibtex-throw: true\n\ncite:[Nobody99]\n";
    let mut parsed = acdc_parser::parse(source, &acdc_parser::Options::default()).expect("parse");
    let processor = Processor::new(Options::builder().base_dir(directory.path()).build());
    let mut warnings = Vec::new();
    let outcome = parsed
        .with_document_mut(|document, arena| processor.process(document, arena, &mut warnings));
    assert!(outcome.is_err(), "an unknown key should abort the run");
}

#[test]
fn the_macro_names_the_database_when_no_attribute_does() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("other.bib"), DATABASE).expect("write database");
    let (lines, warnings) = convert_in(
        directory.path(),
        "= Paper\n\ncite:[Lane12a]\n\nbibliography::other.bib[apa]\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(lines[0], "[.citation]#(<<Lane12a,Lane, 2000>>)#");
    assert_eq!(
        lines[1],
        "[[Lane12a]]Lane, P. (2000). _Book title_. Publisher."
    );
}

#[test]
fn a_named_database_is_relative_to_where_the_command_runs() {
    // What asciidoctor-bibtex does: the path is opened as written, so
    // `sub/references.bib` is right for a command run one level up.
    let directory = tempfile::tempdir().expect("temporary directory");
    let document = directory.path().join("sub");
    fs::create_dir(&document).expect("create directory");
    fs::write(document.join("references.bib"), DATABASE).expect("write database");
    let (lines, warnings) = convert_from(
        &document,
        directory.path(),
        "= Paper\n:bibtex-file: sub/references.bib\n\ncite:[Lane12a]\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(lines[0], "[.citation]#[<<Lane12a,1>>]#");
}

#[test]
fn a_named_database_is_also_looked_for_beside_the_document() {
    // The gem fails here, so accepting it changes nothing that already
    // works — it just lets the same document build from anywhere.
    let directory = tempfile::tempdir().expect("temporary directory");
    let document = directory.path().join("sub");
    fs::create_dir(&document).expect("create directory");
    fs::write(document.join("references.bib"), DATABASE).expect("write database");
    let (lines, warnings) = convert_from(
        &document,
        directory.path(),
        "= Paper\n:bibtex-file: references.bib\n\ncite:[Lane12a]\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(lines[0], "[.citation]#[<<Lane12a,1>>]#");
}

#[test]
fn a_document_that_names_no_database_uses_the_one_beside_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let document = directory.path().join("sub");
    fs::create_dir(&document).expect("create directory");
    fs::write(document.join("references.bib"), DATABASE).expect("write database");
    let (lines, warnings) =
        convert_from(&document, directory.path(), "= Paper\n\ncite:[Lane12a]\n");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(lines[0], "[.citation]#[<<Lane12a,1>>]#");
}

#[test]
fn a_document_that_cites_nothing_needs_no_database() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let (lines, warnings) = convert_in(directory.path(), "= Paper\n\nNothing to cite here.\n");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(lines[0], "Nothing to cite here.");
}

#[test]
fn a_latex_format_emits_passthroughs_for_a_latex_toolchain() {
    let source = "= Paper\n:bibtex-file: references.bib\n:bibtex-format: biblatex\n\n\
                  cite:[Lane12a] citenp:[Lane12b]\n\nbibliography::[]\n";
    let (lines, _) = convert(source);
    assert_eq!(lines[0], "\\parencite{Lane12a} \\textcite{Lane12b}");
    assert_eq!(lines[1], "\\printbibliography");
}
