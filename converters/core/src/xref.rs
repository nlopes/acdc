//! Automatic cross-reference display-text resolution.

use std::{borrow::Cow, cell::Cell, rc::Rc};

use acdc_parser::{
    ContentAttribute, InlineNode, XrefCaptionLabel, XrefSignifier, XrefStyle,
    ast::{CaptionRef, ContentRef, CrossReferenceRef, InlineKind, ReferenceRef},
};

/// Display content for an automatic cross-reference.
#[derive(Debug)]
pub enum XrefDisplay<'r, Content> {
    /// The target's title, to render through the converter's inline pipeline.
    /// Backends that give a title a house style (manpage upper-cases a level-1
    /// section) apply it to this arm only.
    ///
    /// The scope marks the resolution as in progress: hold it while rendering
    /// the nodes so that a cross-reference inside them falls back to `[id]`.
    Title(Content, XrefScope<'r>),
    /// The target's reference label, rendered through the converter's inline
    /// pipeline as written. Carries the same scope as [`XrefDisplay::Title`].
    Label(Content, XrefScope<'r>),
    /// A caption label and number or custom prefix, without the target title.
    ShortCaption(String),
    /// A parsed custom prefix, with its final unformatted period and space omitted.
    /// Render it through [`with_caption_prefix`] while holding the scope.
    ShortCustomCaption(Content, XrefScope<'r>),
    /// A parsed custom prefix followed by the target title in quotation marks.
    /// Render the prefix through [`with_caption_prefix`] while holding the scope.
    FullCustomCaption(Content, Content, XrefScope<'r>),
    /// A caption prefix and the target title in quotation marks. A numbered
    /// section other than a chapter or appendix takes this form too, as in
    /// `Section 1.1, "Title"`.
    FullCaption(String, Content, XrefScope<'r>),
    /// A chapter or appendix title in emphasis, optionally preceded by its
    /// signifier and number, as in `Chapter 2, _Title_` for a full reference.
    Emphasized(Option<String>, Content, XrefScope<'r>),
    /// An inter-document target as written, such as `other.adoc#section`.
    External(String),
    /// The literal `[id]` fallback for a target that is in the catalog but has
    /// no reference text. A link to the target still resolves.
    Fallback(String),
    /// The literal `[id]` fallback for a target that is absent from the
    /// catalog. No anchor exists for it, so a backend whose links must resolve
    /// to a real anchor (a Typst label) renders the text alone.
    Unresolved(String),
    /// The literal `[id]` fallback for a cross-reference inside another
    /// cross-reference's text. Links do not nest, so no backend links this.
    Nested(String),
}

/// Guards cross-reference resolution against unbounded recursion.
///
/// A target's reference text can hold a cross-reference of its own — a block
/// title such as `.See <<a>> again`, or two titles that reference each other.
/// Resolving those through the same path would recurse until the stack
/// overflows, so while one resolution is in progress every nested
/// cross-reference resolves to [`XrefDisplay::Nested`]. Asciidoctor guards the
/// same case and renders the inner reference as `[refid]`.
///
/// Clones share one flag: converters clone their processor freely — per inline
/// node, and for the sub-visitors that render into a buffer — and a nested
/// render must see the resolution its parent started.
#[derive(Clone, Debug, Default)]
pub struct XrefGuard(Rc<Cell<bool>>);

impl XrefGuard {
    /// Whether a resolution is in progress.
    #[must_use]
    pub fn is_resolving(&self) -> bool {
        self.0.get()
    }

    /// Open a resolution, which the returned scope closes when it drops.
    fn enter(&self) -> XrefScope<'_> {
        self.0.set(true);
        XrefScope(self)
    }
}

/// Marks a cross-reference resolution as in progress for as long as it lives.
///
/// Hold this scope while rendering the returned content to prevent nested
/// resolution. Dropping it enables automatic resolution again.
#[derive(Debug)]
pub struct XrefScope<'g>(&'g XrefGuard);

impl Drop for XrefScope<'_> {
    fn drop(&mut self) {
        self.0.0.set(false);
    }
}

/// The default reference label or title stored in the target catalog.
/// [`resolve_reference`] also selects a document label captured at the occurrence.
#[must_use]
pub fn reference_text(reference: ReferenceRef<'_>) -> Option<ContentRef<'_>> {
    reference.xreflabel().or_else(|| reference.title())
}

/// Resolve an empty cross-reference's display content.
///
/// Accepts typed handles and references to cross-reference owners. The result
/// borrows the catalog or the occurrence's retained document label. Keep both
/// alive while rendering the result; the guard has an independent lifetime.
///
/// Explicit reference labels take precedence over caption styles and target
/// titles. Numbered sections honor the style with their word and number, as
/// in `Section 1.1`; an unnumbered one is referenced by its title under every
/// style. Selecting a style emphasizes chapter and appendix titles even without
/// a number. Empty custom captions fall back to the title. Other custom captions
/// omit one final unformatted period and space in short and full references.
/// Formatted prefixes retain their prepared inline content. Captioned targets
/// honor the style; table, example, and listing references can override the
/// target label with the label recorded at the reference position. Unknown
/// local and untitled targets fall back to `[id]`,
/// matching Asciidoctor, and so does a reference that `guard` reports as nested
/// inside another one's text. Inter-document targets are returned separately
/// for backend-specific links, even if the local catalog contains the same ID.
#[must_use]
pub fn resolve_reference<'doc, 'guard>(
    reference: Option<ReferenceRef<'doc>>,
    xref: impl Into<CrossReferenceRef<'doc>>,
    guard: &'guard XrefGuard,
) -> XrefDisplay<'guard, ContentRef<'doc>> {
    let xref = xref.into();
    resolve_content(
        reference,
        reference.and_then(ReferenceRef::title),
        occurrence_label(reference, xref),
        xref,
        guard,
    )
}

/// Resolve reference text as local content, regardless of the occurrence's locality.
///
/// Manpages use matching local labels even for interdocument syntax. This keeps
/// their display policy separate from the parser's retained target classification.
/// External `.adoc` paths use their stem as the catalog key and fallback text.
/// Empty reference labels use the target title or the ID fallback.
/// Select the catalog entry with [`local_reference_id`].
/// Label precedence, styles, fallbacks, and recursion guards follow
/// [`resolve_reference`]; the occurrence remains unchanged.
#[must_use]
pub fn resolve_local_reference<'doc, 'guard>(
    reference: Option<ReferenceRef<'doc>>,
    xref: impl Into<CrossReferenceRef<'doc>>,
    guard: &'guard XrefGuard,
) -> XrefDisplay<'guard, ContentRef<'doc>> {
    let xref = xref.into();
    let target = local_reference_id(xref);
    resolve_local_content(
        reference,
        reference.and_then(ReferenceRef::title),
        occurrence_label(reference, xref).filter(|label| !label.is_empty()),
        &target,
        xref,
        guard,
    )
}

pub(crate) fn reference_text_at<'doc>(
    reference: ReferenceRef<'doc>,
    xref: CrossReferenceRef<'doc>,
) -> Option<ContentRef<'doc>> {
    occurrence_label(Some(reference), xref).or_else(|| reference.title())
}

fn occurrence_label<'doc>(
    reference: Option<ReferenceRef<'doc>>,
    xref: CrossReferenceRef<'doc>,
) -> Option<ContentRef<'doc>> {
    if reference.is_some_and(ReferenceRef::is_document)
        && let Some(label) = xref.document_reftext()
    {
        return (!label.is_empty()).then_some(label);
    }
    reference.and_then(ReferenceRef::xreflabel)
}

/// Catalog key for displaying a reference without an interdocument link.
///
/// External `.adoc` paths omit the source suffix and retain nonempty fragments.
/// A trailing `#` does not contribute to the catalog key.
/// Local IDs keep their spelling, including filename-like IDs. The occurrence's
/// target and locality remain unchanged.
#[must_use]
pub fn local_reference_id(xref: CrossReferenceRef<'_>) -> Cow<'_, str> {
    let target = xref.target();
    if xref.target_is_local() {
        return Cow::Borrowed(target);
    }
    let (path, fragment) = target
        .split_once('#')
        .map_or((target, None), |(path, fragment)| (path, Some(fragment)));
    let stem = path.strip_suffix(".adoc").unwrap_or(path);
    match fragment.filter(|fragment| !fragment.is_empty()) {
        None => Cow::Borrowed(stem),
        Some(_) if stem == path => Cow::Borrowed(target),
        Some(fragment) => Cow::Owned(format!("{stem}#{fragment}")),
    }
}

fn resolve_content<'doc, 'guard>(
    reference: Option<ReferenceRef<'doc>>,
    title: Option<ContentRef<'doc>>,
    label: Option<ContentRef<'doc>>,
    xref: CrossReferenceRef<'_>,
    guard: &'guard XrefGuard,
) -> XrefDisplay<'guard, ContentRef<'doc>> {
    let target = xref.target();
    if !xref.target_is_local() {
        return XrefDisplay::External(target.to_string());
    }
    resolve_local_content(reference, title, label, target, xref, guard)
}

fn resolve_local_content<'doc, 'guard>(
    reference: Option<ReferenceRef<'doc>>,
    title: Option<ContentRef<'doc>>,
    label: Option<ContentRef<'doc>>,
    target: &str,
    xref: CrossReferenceRef<'_>,
    guard: &'guard XrefGuard,
) -> XrefDisplay<'guard, ContentRef<'doc>> {
    let fallback = if target.is_empty() { "^top" } else { target };
    let Some(reference) = reference else {
        return XrefDisplay::Unresolved(format!("[{fallback}]"));
    };
    if guard.is_resolving() {
        return XrefDisplay::Nested(format!("[{fallback}]"));
    }
    if let Some(label) = label {
        XrefDisplay::Label(label, guard.enter())
    } else if let (Some(title), Some(name), Some(number)) =
        (title, reference.section_name(), reference.section_number())
        && matches!(xref.xrefstyle(), XrefStyle::Short | XrefStyle::Full)
    {
        let prefix = section_prefix(name, number, xref.signifier());
        if xref.xrefstyle() == XrefStyle::Short {
            XrefDisplay::ShortCaption(prefix)
        } else if matches!(name, "chapter" | "appendix") {
            XrefDisplay::Emphasized(Some(prefix), title, guard.enter())
        } else {
            XrefDisplay::FullCaption(prefix, title, guard.enter())
        }
    } else if let Some(title) = title
        && xref.xrefstyle() != XrefStyle::Default
        && matches!(reference.section_name(), Some("chapter" | "appendix"))
    {
        XrefDisplay::Emphasized(None, title, guard.enter())
    } else if let (Some(title), Some(CaptionRef::CustomContent(prefix))) =
        (title, reference.caption())
    {
        let empty = crate::InlineTextTransform::default()
            .rendered()
            .content_to_string(prefix)
            .is_empty();
        if !empty && xref.xrefstyle() == XrefStyle::Short {
            XrefDisplay::ShortCustomCaption(prefix, guard.enter())
        } else if !empty && xref.xrefstyle() == XrefStyle::Full {
            XrefDisplay::FullCustomCaption(prefix, title, guard.enter())
        } else {
            XrefDisplay::Title(title, guard.enter())
        }
    } else if let (Some(title), Some(prefix)) = (
        title,
        reference
            .caption()
            .and_then(|caption| caption_prefix(caption, xref)),
    ) {
        if xref.xrefstyle() == XrefStyle::Short {
            XrefDisplay::ShortCaption(prefix)
        } else if xref.xrefstyle() == XrefStyle::Full {
            XrefDisplay::FullCaption(prefix, title, guard.enter())
        } else {
            XrefDisplay::Title(title, guard.enter())
        }
    } else if let Some(title) = title {
        XrefDisplay::Title(title, guard.enter())
    } else {
        XrefDisplay::Fallback(format!("[{fallback}]"))
    }
}

/// A numbered section's word and number: `Section 1.1`, or the number alone
/// when the section's `<name>-refsig` was unset where the reference is written.
///
/// An empty refsig still leaves the space before the number, which is what
/// Asciidoctor prints for one.
fn section_prefix(name: &str, number: &str, signifier: XrefSignifier<'_>) -> String {
    let word = match signifier {
        XrefSignifier::AtReference(word) => Some(word),
        XrefSignifier::Omitted => None,
        XrefSignifier::Standard | _ => match name {
            "part" => Some("Part"),
            "chapter" => Some("Chapter"),
            "section" => Some("Section"),
            "appendix" => Some("Appendix"),
            _ => None,
        },
    };
    match word {
        Some(word) => format!("{word} {number}"),
        None => number.to_string(),
    }
}

fn caption_prefix(caption: CaptionRef<'_>, xref: CrossReferenceRef<'_>) -> Option<String> {
    match caption {
        CaptionRef::Numbered {
            label,
            number: Some(number),
            ..
        } => {
            let label = if let XrefCaptionLabel::AtReference(label) = xref.caption_label() {
                label
            } else if xref.caption_label() == XrefCaptionLabel::NumberOnly {
                ""
            } else {
                label
            };
            if label.is_empty() {
                Some(number.to_string())
            } else {
                Some(format!("{label} {number}"))
            }
        }
        CaptionRef::Custom(prefix) => {
            (!prefix.is_empty()).then(|| prefix.strip_suffix(". ").unwrap_or(prefix).to_string())
        }
        CaptionRef::CustomContent(_) | CaptionRef::Numbered { .. } | CaptionRef::Unnumbered | _ => {
            None
        }
    }
}

/// Render a custom reference prefix without one final unformatted period and space.
///
/// Formatting, raw substitutions, identity and source spans remain intact. The
/// original content is unchanged; punctuation inside formatting remains visible.
pub fn with_caption_prefix<R>(
    content: ContentRef<'_>,
    render: impl FnOnce(ContentRef<'_>) -> R,
) -> R {
    let Some((start, prefix)) = caption_suffix_start(content) else {
        return render(content);
    };
    let mut nodes = ContentAttribute::from(content).into_inlines();
    if prefix.is_empty() {
        nodes.truncate(start);
    } else {
        nodes.truncate(start + 1);
        if let Some(InlineNode::PlainText(node)) = nodes.last_mut() {
            node.replace_text(prefix);
        } else if let Some(InlineNode::RawText(node)) = nodes.last_mut() {
            node.replace_text(prefix);
        }
    }
    render(ContentRef::from(&ContentAttribute::new(nodes)))
}

fn caption_suffix_start(content: ContentRef<'_>) -> Option<(usize, &str)> {
    let mut suffix = ". ";
    for (index, node) in content.inlines().enumerate().rev() {
        let text = if let InlineKind::Plain(text) = node.kind() {
            text.text()
        } else if let InlineKind::Raw(text) = node.kind()
            && text.id().is_none()
            && text.roles().is_empty()
        {
            text.text()
        } else {
            return None;
        };
        if let Some(prefix) = text.strip_suffix(suffix) {
            return Some((index, prefix));
        }
        suffix = suffix.strip_suffix(text)?;
    }
    None
}

/// Map an inter-document cross-reference to a backend target and fallback text.
///
/// Source-document paths use `output_extension`, without a leading dot. A
/// fragment remains in the link target but is omitted from the visible
/// fallback.
#[must_use]
pub fn interdocument_xref(target: &str, output_extension: &str) -> Option<(String, String)> {
    map_interdocument_xref(target, |stem| format!("{stem}.{output_extension}"))
}

/// Map a source-document cross-reference using an exact output suffix.
///
/// The suffix includes any leading punctuation and may be empty. Existing
/// non-source extensions stay unchanged. Fragments remain in the destination
/// and are omitted from the fallback text.
#[must_use]
pub fn interdocument_xref_with_suffix(
    target: &str,
    output_suffix: &str,
) -> Option<(String, String)> {
    map_interdocument_xref(target, |stem| format!("{stem}{output_suffix}"))
}

fn map_interdocument_xref(
    target: &str,
    convert_stem: impl FnOnce(&str) -> String,
) -> Option<(String, String)> {
    if !is_interdocument_target(target) {
        return None;
    }

    let (path, fragment) = match target.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (target, None),
    };
    let stem = source_document_stem(path, fragment.is_some());
    let display = stem.map_or_else(|| path.to_string(), convert_stem);
    let destination = fragment.map_or_else(
        || display.clone(),
        |fragment| format!("{display}#{fragment}"),
    );
    Some((destination, display))
}

fn is_interdocument_target(target: &str) -> bool {
    match target.split_once('#') {
        Some((path, _)) => !path.is_empty(),
        None => target.contains(['.', ':']),
    }
}

fn source_document_stem(path: &str, has_fragment: bool) -> Option<&str> {
    const EXTENSIONS: [&str; 5] = [".adoc", ".asciidoc", ".asc", ".ad", ".txt"];
    if !has_fragment {
        return path.strip_suffix(".adoc");
    }
    EXTENSIONS
        .iter()
        .find_map(|extension| path.strip_suffix(extension))
        .or_else(|| (!path.rsplit('/').next().unwrap_or(path).contains('.')).then_some(path))
}

#[cfg(test)]
mod tests {
    use acdc_parser::{
        Block, CrossReference, Document, Error, InlineMacro, InlineNode, Location, Options,
        ParseResult, SectionKind, XrefCaptionLabel, XrefSignifier, XrefStyle,
        ast::{BlockKind, BlockRef, CrossReferenceRef, InlineKind, InlineRef},
        parse,
    };

    use super::{
        XrefDisplay, XrefGuard, interdocument_xref, interdocument_xref_with_suffix,
        local_reference_id, resolve_local_reference, resolve_reference,
    };

    fn resolve_xref<'doc, 'guard>(
        reference: Option<&'doc acdc_parser::Reference<'doc>>,
        xref: impl Into<acdc_parser::ast::CrossReferenceRef<'doc>>,
        guard: &'guard XrefGuard,
    ) -> XrefDisplay<'guard, acdc_parser::ast::ContentRef<'doc>> {
        resolve_reference(
            reference.map(acdc_parser::ast::ReferenceRef::from),
            xref,
            guard,
        )
    }

    /// Parse a document whose catalog holds `labelled` (an explicit label),
    /// `titled` (a title), and `untitled` (neither).
    fn catalog() -> Result<ParseResult, Error> {
        let input = "= Doc\n\n\
             Some [[labelled,A label]]text.\n\n\
             [[titled]]\n\
             .A title\n\
             ====\n\
             body\n\
             ====\n\n\
             [[untitled]]\n\
             ====\n\
             body\n\
             ====\n";
        parse(input, &Options::default())
    }

    fn xref(target: &'static str, style: XrefStyle) -> CrossReference<'static> {
        let mut xref = CrossReference::new(target, Location::default());
        xref.set_xrefstyle(style);
        xref
    }

    /// A book with a numbered chapter, a section inside it and an appendix,
    /// plus an unnumbered one before sectnums is turned on.
    fn book() -> Result<ParseResult, Error> {
        let input = "= Book\n:doctype: book\n\n[preface]\n== P\n\n\
             [[plain]]\n== Plain\n\nx\n\n:sectnums:\n\n\
             [[ch]]\n== Chap\n\n[[sec]]\n=== Sec\n\nx\n\n\
             [appendix]\n[[app]]\n== App\n\nx\n";
        parse(input, &Options::default())
    }

    fn styled(
        target: &'static str,
        style: XrefStyle,
        signifier: XrefSignifier<'static>,
    ) -> CrossReference<'static> {
        let mut xref = xref(target, style);
        xref.set_signifier(signifier);
        xref
    }

    fn cloned_document<'a>(document: &'a Document<'a>) -> Document<'a> {
        let mut cloned = Document::from(acdc_parser::ast::DocumentRef::from(document));
        cloned.set_header(None);
        cloned
    }

    fn paragraph_xrefs<'a>(blocks: &[Block<'a>]) -> Vec<CrossReference<'a>> {
        let mut references = Vec::new();
        for block in blocks {
            if let Block::Section(section) = block {
                let children: Vec<_> = section.clone().take_blocks().collect();
                references.extend(paragraph_xrefs(&children));
            } else if let Block::Paragraph(paragraph) = block {
                references.extend(paragraph.clone().take_content().filter_map(|inline| {
                    if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline {
                        Some(*xref)
                    } else {
                        None
                    }
                }));
            }
        }
        references
    }

    fn short_prefixes(document: &Document<'_>) -> Vec<String> {
        let mut snapshot = Document::from(acdc_parser::ast::DocumentRef::from(document));
        let blocks: Vec<_> = snapshot.take_blocks().collect();
        let footnotes: Vec<_> = snapshot.take_footnotes().collect();
        let references: std::collections::HashMap<_, _> = snapshot.take_references().collect();
        let mut xrefs = paragraph_xrefs(&blocks);
        for mut footnote in footnotes {
            xrefs.extend(footnote.take_content().into_iter().filter_map(|inline| {
                if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline {
                    Some(*xref)
                } else {
                    None
                }
            }));
        }
        xrefs
            .into_iter()
            .map(|xref| {
                let mut xref = xref.clone();
                // Include references whose style was basic when parsed.
                xref.set_xrefstyle(XrefStyle::Short);
                let guard = XrefGuard::default();
                let display = resolve_xref(
                    references.get(CrossReferenceRef::from(&xref).target()),
                    &xref,
                    &guard,
                );
                if let XrefDisplay::ShortCaption(prefix) = display {
                    prefix
                } else {
                    format!("unexpected reference: {display:?}")
                }
            })
            .collect()
    }

    #[test]
    fn renumbered_signifiers_match_reparsed_source() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse(
            include_str!("../../../acdc-parser/fixtures/tests/xref_signifier_renumber.adoc"),
            &Options::default(),
        )?;
        let parsed_document = acdc_parser::Document::from(parsed.document());
        let expected = parse(
            include_str!("../../../acdc-parser/fixtures/tests/xref_signifier_appendix.adoc"),
            &Options::default(),
        )?;
        let expected_document = acdc_parser::Document::from(expected.document());
        assert_eq!(
            short_prefixes(&expected_document),
            [
                "Early Appendix A",
                "Early Appendix A",
                "Later Appendix A",
                " A",
                "A",
                "Final Appendix A",
                "Early Appendix A",
            ]
        );
        let mut document = cloned_document(&parsed_document);
        let mut blocks: Vec<_> = document.take_blocks().collect();
        for block in &mut blocks {
            if let Block::Section(section) = block
                && section.id() == "target"
            {
                let mut metadata = section.clone_metadata();
                metadata.style = SectionKind::Appendix.as_style();
                section.set_metadata(metadata)?;
            }
        }
        document.set_blocks(blocks);
        document.renumber_sections();
        assert_eq!(
            short_prefixes(&document),
            short_prefixes(&expected_document)
        );
        document.renumber_sections();
        assert_eq!(
            short_prefixes(&document),
            short_prefixes(&expected_document)
        );

        let mut blocks: Vec<_> = document.take_blocks().collect();
        for block in &mut blocks {
            if let Block::Section(section) = block
                && section.id() == "target"
            {
                let mut metadata = section.clone_metadata();
                metadata.style = SectionKind::Normal.as_style();
                section.set_metadata(metadata)?;
            }
        }
        document.set_blocks(blocks);
        document.renumber_sections();
        assert_eq!(short_prefixes(&document), short_prefixes(&parsed_document));
        Ok(())
    }

    #[test]
    fn signifier_overrides_survive_category_changes() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse(
            include_str!("../../../acdc-parser/fixtures/tests/xref_signifier_renumber.adoc"),
            &Options::default(),
        )?;
        let parsed_document = acdc_parser::Document::from(parsed.document());
        for (signifier, expected) in [
            (XrefSignifier::Standard, "Appendix A"),
            (XrefSignifier::AtReference("Chapter"), "Chapter A"),
            (XrefSignifier::AtReference("Custom"), "Custom A"),
            (XrefSignifier::AtReference(""), " A"),
            (XrefSignifier::Omitted, "A"),
        ] {
            let mut document = cloned_document(&parsed_document);
            let mut footnotes: Vec<_> = document.take_footnotes().collect();
            for footnote in &mut footnotes {
                let mut content = footnote.take_content();
                for inline in &mut content {
                    if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline {
                        xref.set_signifier(signifier);
                    }
                }
                footnote.replace_content(content);
            }
            let mut blocks: Vec<_> = document.take_blocks().collect();
            for block in &mut blocks {
                if let Block::Section(section) = block
                    && section.id() == "target"
                {
                    let mut metadata = section.clone_metadata();
                    metadata.style = SectionKind::Appendix.as_style();
                    section.set_metadata(metadata)?;
                }
            }
            document.set_footnotes(footnotes);
            document.set_blocks(blocks);
            // Cloning must preserve explicit overrides, including the original word.
            let mut document = cloned_document(&document);
            document.renumber_sections();
            document.renumber_sections();
            assert_eq!(
                short_prefixes(&document).last().map(String::as_str),
                Some(expected)
            );
            assert_eq!(
                short_prefixes(&document).first().map(String::as_str),
                Some("Early Appendix A")
            );
        }
        Ok(())
    }

    /// What a section reference resolves to, as `(form, prefix)`.
    fn section_form(
        target: &'static str,
        style: XrefStyle,
        signifier: XrefSignifier<'static>,
    ) -> Result<(&'static str, String), Error> {
        let parsed = book()?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let references = &parsed_references;
        let guard = XrefGuard::default();
        Ok(
            match resolve_xref(
                references.get(target),
                &styled(target, style, signifier),
                &guard,
            ) {
                XrefDisplay::Title(..) => ("title", String::new()),
                XrefDisplay::ShortCaption(prefix) => ("short", prefix),
                XrefDisplay::FullCaption(prefix, ..) => ("quoted", prefix),
                XrefDisplay::Emphasized(prefix, ..) => ("emphasized", prefix.unwrap_or_default()),
                // Reported as a value so the assertion shows what came back.
                other @ (XrefDisplay::ShortCustomCaption(..)
                | XrefDisplay::FullCustomCaption(..)
                | XrefDisplay::Label(..)
                | XrefDisplay::External(_)
                | XrefDisplay::Fallback(_)
                | XrefDisplay::Unresolved(_)
                | XrefDisplay::Nested(_)) => ("unexpected", format!("{other:?}")),
            },
        )
    }

    #[test]
    fn a_numbered_section_honors_the_style() -> Result<(), Error> {
        let standard = XrefSignifier::Standard;
        assert_eq!(
            section_form("sec", XrefStyle::Basic, standard)?,
            ("title", String::new())
        );
        assert_eq!(
            section_form("sec", XrefStyle::Short, standard)?,
            ("short", "Section 1.1".to_string())
        );
        assert_eq!(
            section_form("sec", XrefStyle::Full, standard)?,
            ("quoted", "Section 1.1".to_string())
        );
        Ok(())
    }

    #[test]
    fn a_special_section_has_no_standard_signifier() -> Result<(), Error> {
        let parsed = parse(
            include_str!(
                "../../html/tests/fixtures/source/html/embedded/book_special_section_numbering_all.adoc"
            ),
            &Options::default(),
        )?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(
                parsed_references.get("_preface"),
                &xref("_preface", XrefStyle::Short),
                &guard,
            ),
            XrefDisplay::ShortCaption(prefix) if prefix == "1"
        ));
        Ok(())
    }

    #[test]
    fn a_chapter_or_appendix_title_is_emphasized_not_quoted() -> Result<(), Error> {
        let standard = XrefSignifier::Standard;
        assert_eq!(
            section_form("ch", XrefStyle::Full, standard)?,
            ("emphasized", "Chapter 1".to_string())
        );
        assert_eq!(
            section_form("app", XrefStyle::Full, standard)?,
            ("emphasized", "Appendix A".to_string())
        );
        assert_eq!(
            section_form("ch", XrefStyle::Short, standard)?,
            ("short", "Chapter 1".to_string())
        );
        Ok(())
    }

    #[test]
    fn an_unnumbered_chapter_keeps_selected_title_emphasis() -> Result<(), Error> {
        assert_eq!(
            section_form("plain", XrefStyle::Default, XrefSignifier::Standard)?,
            ("title", String::new())
        );
        for style in [XrefStyle::Basic, XrefStyle::Short, XrefStyle::Full] {
            assert_eq!(
                section_form("plain", style, XrefSignifier::Standard)?,
                ("emphasized", String::new())
            );
        }
        Ok(())
    }

    #[test]
    fn the_signifier_recorded_at_the_reference_is_used() -> Result<(), Error> {
        assert_eq!(
            section_form(
                "sec",
                XrefStyle::Short,
                XrefSignifier::AtReference("Abschnitt")
            )?,
            ("short", "Abschnitt 1.1".to_string())
        );
        assert_eq!(
            section_form("sec", XrefStyle::Short, XrefSignifier::Omitted)?,
            ("short", "1.1".to_string())
        );
        // An empty refsig still leaves the space before the number.
        assert_eq!(
            section_form("sec", XrefStyle::Short, XrefSignifier::AtReference(""))?,
            ("short", " 1.1".to_string())
        );
        Ok(())
    }

    #[test]
    fn label_takes_precedence_over_title() -> Result<(), Error> {
        // `[[labelled,A label]]` sits on an inline anchor, which has no title;
        // a section with both is covered by the html and terminal fixtures.
        let parsed = catalog()?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let references = &parsed_references;
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(
                references.get("labelled"),
                &xref("labelled", XrefStyle::Basic),
                &guard
            ),
            XrefDisplay::Label(..)
        ));
        Ok(())
    }

    #[test]
    fn title_resolves_when_there_is_no_label() -> Result<(), Error> {
        let parsed = catalog()?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let references = &parsed_references;
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(
                references.get("titled"),
                &xref("titled", XrefStyle::Basic),
                &guard
            ),
            XrefDisplay::Title(..)
        ));
        Ok(())
    }

    #[test]
    fn captioned_target_resolves_short_and_full_styles() -> Result<(), Error> {
        let parsed = catalog()?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let references = &parsed_references;
        let guard = XrefGuard::default();

        assert!(matches!(
            resolve_xref(
                references.get("titled"),
                &xref("titled", XrefStyle::Short),
                &guard
            ),
            XrefDisplay::ShortCaption(prefix) if prefix == "Example 1"
        ));
        assert!(matches!(
            resolve_xref(
                references.get("titled"),
                &xref("titled", XrefStyle::Full),
                &guard
            ),
            XrefDisplay::FullCaption(prefix, title, _scope)
                if prefix == "Example 1"
                    && matches!(title.inlines().map(acdc_parser::ast::InlineRef::kind).collect::<Vec<_>>().as_slice(), [acdc_parser::ast::InlineKind::Plain(text)] if text.text() == "A title")
        ));
        let mut number_only = xref("titled", XrefStyle::Short);
        number_only.set_caption_label(XrefCaptionLabel::NumberOnly);
        assert!(matches!(
            resolve_xref(references.get("titled"), &number_only, &guard),
            XrefDisplay::ShortCaption(prefix) if prefix == "1"
        ));
        Ok(())
    }

    #[test]
    fn caption_labels_follow_reference_and_target_source_order() -> Result<(), Error> {
        let parsed = parse(
            ":figure-caption: BeforeFigure\n:table-caption: BeforeTable\n:example-caption: BeforeExample\n:listing-caption: BeforeListing\n:xrefstyle: short\n\nForward: <<figure>>; <<table>>; <<example>>; <<listing>>.\n\n:figure-caption: TargetFigure\n:table-caption: TargetTable\n:example-caption: TargetExample\n:listing-caption: TargetListing\n\n[[figure]]\n.Figure title\nimage::figure.svg[]\n\n[[table]]\n.Table title\n|===\n|Cell\n|===\n\n[[example]]\n.Example title\n====\nBody.\n====\n\n[[listing]]\n.Listing title\n----\nbody\n----\n\n:figure-caption: AfterFigure\n:table-caption: AfterTable\n:example-caption: AfterExample\n:listing-caption: AfterListing\n\nBackward: <<figure>>; <<table>>; <<example>>; <<listing>>.\n",
            &Options::default(),
        )?;
        let parsed_document = acdc_parser::Document::from(parsed.document());
        let mut parsed_document = parsed_document;
        let references: std::collections::HashMap<_, _> =
            parsed_document.take_references().collect();
        let blocks: Vec<_> = parsed_document.take_blocks().collect();
        let guard = XrefGuard::default();
        let prefixes = blocks
            .iter()
            .filter_map(|block| {
                let Block::Paragraph(paragraph) = block else {
                    return None;
                };
                Some(paragraph)
            })
            .flat_map(|paragraph| paragraph.clone().take_content())
            .filter_map(|inline| {
                let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                    return None;
                };
                let XrefDisplay::ShortCaption(prefix) = resolve_xref(
                    references.get(CrossReferenceRef::from(xref.as_ref()).target()),
                    xref.as_ref(),
                    &guard,
                ) else {
                    return None;
                };
                Some(prefix)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            prefixes,
            [
                "TargetFigure 1",
                "BeforeTable 1",
                "BeforeExample 1",
                "BeforeListing 1",
                "TargetFigure 1",
                "AfterTable 1",
                "AfterExample 1",
                "AfterListing 1",
            ]
        );
        Ok(())
    }

    #[test]
    fn custom_and_disabled_captions_resolve_like_asciidoctor() -> Result<(), Error> {
        let parsed = parse(
            ":figure-caption!:\n\n[[disabled]]\n.Disabled title\nimage::disabled.svg[]\n\n[[custom]]\n.Custom title\n[caption=\"Exhibit: \"]\nimage::custom.svg[]\n",
            &Options::default(),
        )?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let references = &parsed_references;
        let guard = XrefGuard::default();

        assert!(matches!(
            resolve_xref(
                references.get("disabled"),
                &xref("disabled", XrefStyle::Short),
                &guard
            ),
            XrefDisplay::Title(..)
        ));
        assert!(matches!(
            resolve_xref(
                references.get("custom"),
                &xref("custom", XrefStyle::Short),
                &guard
            ),
            XrefDisplay::ShortCustomCaption(prefix, _scope)
                if crate::InlineTextTransform::default().content_to_string(prefix) == "Exhibit: "
        ));
        assert!(matches!(
            resolve_xref(
                references.get("custom"),
                &xref("custom", XrefStyle::Full),
                &guard
            ),
            XrefDisplay::FullCustomCaption(prefix, ..)
                if crate::InlineTextTransform::default().content_to_string(prefix) == "Exhibit: "
        ));
        Ok(())
    }

    #[test]
    fn untitled_target_falls_back_to_its_id() -> Result<(), Error> {
        let parsed = catalog()?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let references = &parsed_references;
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(
                references.get("untitled"),
                &xref("untitled", XrefStyle::Basic),
                &guard
            ),
            XrefDisplay::Fallback(text) if text == "[untitled]"
        ));
        Ok(())
    }

    #[test]
    fn absent_target_is_unresolved() {
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(None, &xref("no-such-id", XrefStyle::Basic), &guard),
            XrefDisplay::Unresolved(text) if text == "[no-such-id]"
        ));
    }

    #[test]
    fn external_reference_ignores_a_matching_local_catalog_entry() -> Result<(), Error> {
        let parsed = catalog()?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let mut external = xref("titled", XrefStyle::Basic);
        external.set_target_is_local(false);
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(parsed_references.get("titled"), &external, &guard),
            XrefDisplay::External(target) if target == "titled"
        ));
        Ok(())
    }

    #[test]
    fn local_display_preserves_label_precedence_and_occurrence_locality()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = catalog()?;
        let references = parsed.document().references();
        for (target, expected) in [("labelled", "A label"), ("titled", "A title")] {
            let mut external = xref(target, XrefStyle::Full);
            external.set_target_is_local(false);
            let guard = XrefGuard::default();
            assert!(
                matches!(resolve_reference(references.get(target), &external, &guard), XrefDisplay::External(value) if value == target)
            );
            let display = resolve_local_reference(references.get(target), &external, &guard);
            let content = if let XrefDisplay::Label(content, _) = &display {
                assert_eq!(target, "labelled");
                *content
            } else if let XrefDisplay::FullCaption(prefix, content, _) = &display {
                assert_eq!(target, "titled");
                assert_eq!(prefix, "Example 1");
                *content
            } else {
                return Err(format!("expected local content: {display:?}").into());
            };
            assert_eq!(
                crate::InlineTextTransform::default().content_to_string(content),
                expected
            );
            assert!(guard.is_resolving());
            assert!(
                matches!(resolve_local_reference(references.get(target), &external, &guard), XrefDisplay::Nested(value) if value == format!("[{target}]"))
            );
            drop(display);
            assert!(!guard.is_resolving());
            assert!(!CrossReferenceRef::from(&external).target_is_local());
            assert_eq!(
                CrossReferenceRef::from(&external).xrefstyle(),
                XrefStyle::Full
            );
        }
        Ok(())
    }

    #[test]
    fn local_display_keeps_unresolved_and_top_fallbacks() {
        let guard = XrefGuard::default();
        for (target, expected) in [
            ("", "[^top]"),
            ("missing", "[missing]"),
            ("other.adoc#part", "[other#part]"),
        ] {
            let mut external = xref(target, XrefStyle::Basic);
            external.set_target_is_local(false);
            assert!(
                matches!(resolve_local_reference(None, &external, &guard), XrefDisplay::Unresolved(value) if value == expected)
            );
            assert!(!guard.is_resolving());
        }
    }

    #[test]
    fn local_catalog_keys_preserve_locality_and_non_source_extensions() {
        for (target, expected) in [
            ("guide.adoc", "guide"),
            ("guide.adoc#part", "guide#part"),
            ("guide.adoc#", "guide"),
            ("guide.pdf#", "guide.pdf"),
            ("dir/guide.adoc", "dir/guide"),
            ("guide.pdf#part", "guide.pdf#part"),
            ("guide.asciidoc", "guide.asciidoc"),
            ("guide.adoc.bak", "guide.adoc.bak"),
        ] {
            for local in [false, true] {
                let mut occurrence = xref(target, XrefStyle::Basic);
                occurrence.set_target_is_local(local);
                let read = CrossReferenceRef::from(&occurrence);
                assert_eq!(
                    local_reference_id(read),
                    if local { target } else { expected }
                );
                assert_eq!(read.target(), target);
                assert_eq!(read.target_is_local(), local);
            }
        }
    }

    #[test]
    fn file_reference_display_uses_the_stem_catalog_entry() -> Result<(), Box<dyn std::error::Error>>
    {
        for mode in [
            acdc_parser::CompatibilityMode::Asciidoctor,
            acdc_parser::CompatibilityMode::Draft,
        ] {
            let parsed = parse(
                "[#external.adoc,reftext=\"Stem label\"]\nTarget.\n\n[[external.adoc,Exact ID label]]\nOther target.\n\nxref:external.adoc[]\n",
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let references = parsed.document().references();
            let Some(BlockKind::Paragraph(paragraph)) =
                parsed.document().blocks().last().map(BlockRef::kind)
            else {
                return Err("expected reference paragraph".into());
            };
            let Some(InlineKind::CrossReference(occurrence)) =
                paragraph.content().inlines().next().map(InlineRef::kind)
            else {
                return Err("expected cross-reference".into());
            };
            assert_eq!(occurrence.target(), "external.adoc");
            assert!(!occurrence.target_is_local());
            assert!(references.contains("external"));
            assert!(references.contains("external.adoc"));
            let key = local_reference_id(occurrence);
            let reference = references.get(&key);
            let guard = XrefGuard::default();
            assert!(matches!(
                resolve_reference(reference, occurrence, &guard),
                XrefDisplay::External(target) if target == "external.adoc"
            ));
            let display = resolve_local_reference(reference, occurrence, &guard);
            let XrefDisplay::Label(content, _) = display else {
                return Err("expected the stem's reference label".into());
            };
            assert_eq!(
                crate::InlineTextTransform::default().content_to_string(content),
                "Stem label"
            );
            assert_eq!(occurrence.target(), "external.adoc");
            assert!(!occurrence.target_is_local());
        }
        Ok(())
    }

    #[test]
    fn interdocument_target_is_not_an_unresolved_local_id() {
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(
                None,
                &xref("other.adoc#part", XrefStyle::Basic),
                &guard
            ),
            XrefDisplay::External(target) if target == "other.adoc#part"
        ));
    }

    #[test]
    fn interdocument_target_uses_backend_suffix_and_hides_fragment() {
        assert_eq!(
            interdocument_xref("other.adoc#part", "pdf"),
            Some(("other.pdf#part".to_string(), "other.pdf".to_string()))
        );
        assert_eq!(
            interdocument_xref("manual.pdf#part", "html"),
            Some(("manual.pdf#part".to_string(), "manual.pdf".to_string()))
        );
        assert_eq!(interdocument_xref("local-id", "html"), None);
    }

    #[test]
    fn interdocument_suffix_is_literal_and_can_be_empty() {
        for (suffix, file) in [
            (".xhtml", "guide.xhtml"),
            ("", "guide"),
            ("html", "guidehtml"),
        ] {
            for target in ["guide.adoc#part", "guide.asciidoc#part", "guide#part"] {
                assert_eq!(
                    interdocument_xref_with_suffix(target, suffix),
                    Some((format!("{file}#part"), file.to_string()))
                );
            }
            assert_eq!(
                interdocument_xref_with_suffix("guide.adoc", suffix),
                Some((file.to_string(), file.to_string()))
            );
            assert_eq!(
                interdocument_xref_with_suffix("guide.pdf#part", suffix),
                Some(("guide.pdf#part".to_string(), "guide.pdf".to_string()))
            );
        }
        assert_eq!(interdocument_xref_with_suffix("#part", ""), None);
        assert_eq!(interdocument_xref_with_suffix("part", ""), None);
    }

    #[test]
    fn reference_inside_reference_text_is_nested() -> Result<(), Error> {
        let parsed = catalog()?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let references = &parsed_references;
        let guard = XrefGuard::default();

        let occurrence = xref("titled", XrefStyle::Basic);
        let display = resolve_xref(references.get("titled"), &occurrence, &guard);
        assert!(matches!(display, XrefDisplay::Title(..)));
        assert!(guard.is_resolving());
        // While the outer resolution is open, a nested one cannot recurse.
        assert!(matches!(
            resolve_xref(
                references.get("labelled"),
                &xref("labelled", XrefStyle::Basic),
                &guard
            ),
            XrefDisplay::Nested(text) if text == "[labelled]"
        ));

        drop(display);
        assert!(!guard.is_resolving());
        assert!(matches!(
            resolve_xref(
                references.get("labelled"),
                &xref("labelled", XrefStyle::Basic),
                &guard
            ),
            XrefDisplay::Label(..)
        ));
        Ok(())
    }

    #[test]
    fn clones_of_a_guard_share_one_resolution() -> Result<(), Error> {
        let parsed = catalog()?;
        let parsed_references = std::collections::HashMap::from(parsed.document().references());
        let references = &parsed_references;
        let guard = XrefGuard::default();
        let clone = guard.clone();

        let occurrence = xref("titled", XrefStyle::Basic);
        let display = resolve_xref(references.get("titled"), &occurrence, &guard);
        assert!(clone.is_resolving());
        drop(display);
        assert!(!clone.is_resolving());
        Ok(())
    }
}
