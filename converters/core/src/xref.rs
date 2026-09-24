//! Automatic cross-reference display-text resolution.

use std::{cell::Cell, rc::Rc};

use acdc_parser::{
    Caption, CrossReference, InlineNode, Reference, XrefCaptionLabel, XrefSignifier, XrefStyle,
};

/// Display content for an automatic cross-reference.
#[derive(Debug)]
pub enum XrefDisplay<'r, 'a> {
    /// The target's title, to render through the converter's inline pipeline.
    /// Backends that give a title a house style (manpage upper-cases a level-1
    /// section) apply it to this arm only.
    ///
    /// The scope marks the resolution as in progress: hold it while rendering
    /// the nodes so that a cross-reference inside them falls back to `[id]`.
    Title(&'r [InlineNode<'a>], XrefScope<'r>),
    /// The target's reference label, rendered through the converter's inline
    /// pipeline as written. Carries the same scope as [`XrefDisplay::Title`].
    Label(&'r [InlineNode<'a>], XrefScope<'r>),
    /// A caption label and number or custom prefix, without the target title.
    ShortCaption(String),
    /// A caption prefix and the target title in quotation marks. A numbered
    /// section other than a chapter or appendix takes this form too, as in
    /// `Section 1.1, "Title"`.
    FullCaption(String, &'r [InlineNode<'a>], XrefScope<'r>),
    /// A section's word and number followed by its title in emphasis, which
    /// is how Asciidoctor sets a chapter or appendix in a `full` reference:
    /// `Chapter 2, _Title_`.
    FullEmphasized(String, &'r [InlineNode<'a>], XrefScope<'r>),
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
/// Obtained from [`resolve_xref`] together with the nodes to render, so the
/// nodes cannot be rendered without the guard in place.
#[derive(Debug)]
pub struct XrefScope<'g>(&'g XrefGuard);

impl Drop for XrefScope<'_> {
    fn drop(&mut self) {
        self.0.0.set(false);
    }
}

/// The inline nodes a target's reference text is made of, if it has any.
///
/// The precedence is [`resolve_xref`]'s, without the distinctions a rendering
/// backend needs: plain-text extraction treats a label and a title alike.
#[must_use]
pub fn reference_text<'r, 'a>(reference: &'r Reference<'a>) -> Option<&'r [InlineNode<'a>]> {
    match (&reference.xreflabel, &reference.title) {
        (Some(label), _) => Some(label),
        (None, Some(title)) => Some(title.as_ref()),
        (None, None) => None,
    }
}

/// Resolve an empty cross-reference's display content.
///
/// Explicit reference labels take precedence over caption styles and target
/// titles. Numbered sections honor the style with their word and number, as
/// in `Section 1.1`; an unnumbered one is referenced by its title under every
/// style, as in Asciidoctor. Captioned targets honor the style; table, example, and listing
/// references can override the target label with the label recorded at the
/// reference position. Unknown local and untitled targets fall back to `[id]`,
/// matching Asciidoctor, and so does a reference that `guard` reports as nested
/// inside another one's text. Inter-document targets are returned separately
/// for backend-specific links.
#[must_use]
pub fn resolve_xref<'r, 'a>(
    reference: Option<&'r Reference<'a>>,
    xref: &CrossReference<'_>,
    guard: &'r XrefGuard,
) -> XrefDisplay<'r, 'a> {
    let target = xref.target;
    let Some(reference) = reference else {
        return if is_interdocument_target(target) {
            XrefDisplay::External(target.to_string())
        } else {
            XrefDisplay::Unresolved(format!("[{target}]"))
        };
    };
    if guard.is_resolving() {
        return XrefDisplay::Nested(format!("[{target}]"));
    }
    if let Some(label) = &reference.xreflabel {
        XrefDisplay::Label(label, guard.enter())
    } else if let (Some(title), Some(name), Some(number)) = (
        &reference.title,
        reference.section_name(),
        reference.section_number(),
    ) && xref.xrefstyle != XrefStyle::Basic
    {
        let prefix = section_prefix(name, number, xref.signifier);
        if xref.xrefstyle == XrefStyle::Short {
            XrefDisplay::ShortCaption(prefix)
        } else if matches!(name, "chapter" | "appendix") {
            XrefDisplay::FullEmphasized(prefix, title.as_ref(), guard.enter())
        } else {
            XrefDisplay::FullCaption(prefix, title.as_ref(), guard.enter())
        }
    } else if let (Some(title), Some(prefix)) = (
        &reference.title,
        reference
            .caption
            .as_ref()
            .and_then(|caption| caption_prefix(caption, xref)),
    ) {
        if xref.xrefstyle == XrefStyle::Short {
            XrefDisplay::ShortCaption(prefix)
        } else if xref.xrefstyle == XrefStyle::Full {
            XrefDisplay::FullCaption(prefix, title.as_ref(), guard.enter())
        } else {
            XrefDisplay::Title(title.as_ref(), guard.enter())
        }
    } else if let Some(title) = &reference.title {
        XrefDisplay::Title(title.as_ref(), guard.enter())
    } else {
        XrefDisplay::Fallback(format!("[{target}]"))
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

fn caption_prefix(caption: &Caption<'_>, xref: &CrossReference<'_>) -> Option<String> {
    match caption {
        Caption::Numbered {
            label,
            number: Some(number),
            ..
        } => {
            let label = if let XrefCaptionLabel::AtReference(label) = xref.caption_label {
                label
            } else if xref.caption_label == XrefCaptionLabel::NumberOnly {
                ""
            } else {
                label.as_ref()
            };
            if label.is_empty() {
                Some(number.to_string())
            } else {
                Some(format!("{label} {number}"))
            }
        }
        Caption::Custom(prefix) => Some(prefix.to_string()),
        Caption::Numbered { .. } | Caption::Unnumbered | _ => None,
    }
}

/// Map an inter-document cross-reference to a backend target and fallback text.
///
/// Source-document paths use `output_extension`, without a leading dot. A
/// fragment remains in the link target but is omitted from the visible
/// fallback.
#[must_use]
pub fn interdocument_xref(target: &str, output_extension: &str) -> Option<(String, String)> {
    if !is_interdocument_target(target) {
        return None;
    }

    let (path, fragment) = match target.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (target, None),
    };
    let stem = source_document_stem(path, fragment.is_some());
    let display = stem.map_or_else(
        || path.to_string(),
        |stem| format!("{stem}.{output_extension}"),
    );
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
        Block, CrossReference, Error, InlineMacro, InlineNode, Location, Options, ParseResult,
        XrefCaptionLabel, XrefSignifier, XrefStyle, parse,
    };

    use super::{XrefDisplay, XrefGuard, interdocument_xref, resolve_xref};

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
        xref.xrefstyle = style;
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
        xref.signifier = signifier;
        xref
    }

    /// What a section reference resolves to, as `(form, prefix)`.
    fn section_form(
        target: &'static str,
        style: XrefStyle,
        signifier: XrefSignifier<'static>,
    ) -> Result<(&'static str, String), Error> {
        let parsed = book()?;
        let references = &parsed.document().references;
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
                XrefDisplay::FullEmphasized(prefix, ..) => ("emphasized", prefix),
                // Reported as a value so the assertion shows what came back.
                other @ (XrefDisplay::Label(..)
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
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(
                parsed.document().references.get("_preface"),
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
    fn an_unnumbered_section_is_referenced_by_its_title() -> Result<(), Error> {
        for style in [XrefStyle::Basic, XrefStyle::Short, XrefStyle::Full] {
            assert_eq!(
                section_form("plain", style, XrefSignifier::Standard)?,
                ("title", String::new())
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
        let references = &parsed.document().references;
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
        let references = &parsed.document().references;
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
        let references = &parsed.document().references;
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
                    && matches!(title, [acdc_parser::InlineNode::PlainText(text)] if text.content == "A title")
        ));
        let mut number_only = xref("titled", XrefStyle::Short);
        number_only.caption_label = XrefCaptionLabel::NumberOnly;
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
        let references = &parsed.document().references;
        let guard = XrefGuard::default();
        let prefixes = parsed
            .document()
            .blocks
            .iter()
            .filter_map(|block| {
                let Block::Paragraph(paragraph) = block else {
                    return None;
                };
                Some(paragraph)
            })
            .flat_map(|paragraph| paragraph.content.iter())
            .filter_map(|inline| {
                let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                    return None;
                };
                let XrefDisplay::ShortCaption(prefix) =
                    resolve_xref(references.get(xref.target), xref, &guard)
                else {
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
        let references = &parsed.document().references;
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
            XrefDisplay::ShortCaption(prefix) if prefix == "Exhibit: "
        ));
        assert!(matches!(
            resolve_xref(
                references.get("custom"),
                &xref("custom", XrefStyle::Full),
                &guard
            ),
            XrefDisplay::FullCaption(prefix, ..) if prefix == "Exhibit: "
        ));
        Ok(())
    }

    #[test]
    fn untitled_target_falls_back_to_its_id() -> Result<(), Error> {
        let parsed = catalog()?;
        let references = &parsed.document().references;
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
    fn reference_inside_reference_text_is_nested() -> Result<(), Error> {
        let parsed = catalog()?;
        let references = &parsed.document().references;
        let guard = XrefGuard::default();

        let display = resolve_xref(
            references.get("titled"),
            &xref("titled", XrefStyle::Basic),
            &guard,
        );
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
        let references = &parsed.document().references;
        let guard = XrefGuard::default();
        let clone = guard.clone();

        let display = resolve_xref(
            references.get("titled"),
            &xref("titled", XrefStyle::Basic),
            &guard,
        );
        assert!(clone.is_resolving());
        drop(display);
        assert!(!clone.is_resolving());
        Ok(())
    }
}
