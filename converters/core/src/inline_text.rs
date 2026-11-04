//! Plain-text extraction from inline nodes.
//!
//! Headings, captions, and other places that cannot carry markup still have to
//! carry the *text* of what they hold. Every inline node contributes its text
//! here: a link contributes its link text, and a cross-reference contributes the
//! reference text of its target when a catalog is supplied.

use std::{
    borrow::Cow,
    fmt::{self, Write},
};

use acdc_parser::{
    InlineNode, Substitution,
    ast::{ContentRef, CrossReferenceRef, InlineKind, InlineRef, ReferencesRef},
};

use crate::{
    decode_numeric_char_refs,
    substitutions::{Replacements, TextBoundaries, apply_passthrough_replacements_iter},
    xref::reference_text,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextProjection {
    Source,
    Rendered,
}

/// Plain-text extraction policy for inline nodes.
#[derive(Clone, Copy, Debug)]
pub struct InlineTextTransform<'t, 'd> {
    line_break: &'t str,
    decode_char_refs: bool,
    projection: TextProjection,
    text_boundaries: TextBoundaries,
    references: Option<ReferencesRef<'d>>,
    /// Set while extracting a target's reference text, so a cross-reference
    /// inside it falls back to `[id]` instead of recursing. See
    /// [`crate::xref::XrefGuard`], which does the same for rendered output.
    resolving_xref: bool,
}

impl Default for InlineTextTransform<'_, '_> {
    fn default() -> Self {
        Self {
            line_break: " ",
            decode_char_refs: false,
            projection: TextProjection::Source,
            text_boundaries: TextBoundaries::BOTH,
            references: None,
            resolving_xref: false,
        }
    }
}

impl<'t, 'd> InlineTextTransform<'t, 'd> {
    /// Render hard line breaks as `line_break`.
    #[must_use]
    pub fn line_break(mut self, line_break: &'t str) -> Self {
        self.line_break = line_break;
        self
    }

    /// Decode parsed character references and numeric references in raw text.
    ///
    /// Non-HTML backends want the character; HTML keeps the reference.
    /// Explicitly escaped references remain literal.
    #[must_use]
    pub fn decode_char_refs(mut self, decode: bool) -> Self {
        self.decode_char_refs = decode;
        self
    }

    /// Apply the remaining substitutions carried by Raw fragments.
    ///
    /// Resolved plain and verbatim text stay unchanged. The default keeps Raw
    /// source text without applying its remaining substitutions.
    #[must_use]
    pub fn rendered(mut self) -> Self {
        self.text_boundaries = self.text_boundaries.with_ordinary_replacements(false);
        self.projection = TextProjection::Rendered;
        self
    }

    /// Use the enclosing fragment's text context for typography.
    ///
    /// The default treats both ends as paragraph boundaries with no neighbors.
    /// Nested index labels retain this context rather than gaining paragraph edges.
    #[must_use]
    pub fn text_boundaries(mut self, boundaries: TextBoundaries) -> Self {
        self.text_boundaries = boundaries;
        self
    }

    /// Resolve a cross-reference with no text of its own through this catalog,
    /// so it contributes its target's reference text.
    ///
    /// Without a catalog such a reference contributes its target id, which is
    /// all an inter-document reference has.
    #[must_use]
    pub fn references(mut self, references: ReferencesRef<'d>) -> Self {
        self.references = Some(references);
        self
    }

    /// Keep cross-references inside reference text from expanding another title.
    pub(crate) fn in_reference(self) -> Self {
        Self {
            resolving_xref: true,
            ..self
        }
    }

    /// Write plain text extracted from `inlines` into `w`.
    ///
    /// # Errors
    ///
    /// Returns any error produced by the underlying writer.
    pub fn write<W: Write + ?Sized>(self, w: &mut W, inlines: &[InlineNode<'_>]) -> fmt::Result {
        self.write_inlines(w, inlines.iter().map(InlineRef::from))
    }

    /// Write plain text from typed content using this extraction policy.
    ///
    /// # Errors
    ///
    /// Returns any error produced by the underlying writer.
    pub fn write_content<W: Write + ?Sized>(
        self,
        w: &mut W,
        content: ContentRef<'_>,
    ) -> fmt::Result {
        self.write_inlines(w, content.inlines())
    }

    /// Write plain text from a typed inline iterator using this extraction policy.
    ///
    /// For an isolated fragment, supply its surrounding context with
    /// [`Self::text_boundaries`].
    ///
    /// # Errors
    ///
    /// Returns any error produced by the underlying writer.
    pub fn write_inlines<'doc, W: Write + ?Sized>(
        self,
        w: &mut W,
        inlines: impl DoubleEndedIterator<Item = InlineRef<'doc>> + ExactSizeIterator + Clone,
    ) -> fmt::Result {
        let siblings = inlines.clone();
        for (index, node) in inlines.enumerate() {
            let context = Self {
                text_boundaries: self.text_boundaries.for_inline_refs(
                    Some(node),
                    siblings.clone().take(index).rev(),
                    siblings.clone().skip(index + 1),
                ),
                ..self
            };
            context.write_inline(w, node)?;
        }
        Ok(())
    }

    fn write_inline<W: Write + ?Sized>(self, w: &mut W, node: InlineRef<'_>) -> fmt::Result {
        match node.kind() {
            InlineKind::Plain(text) => w.write_str(text.text()),
            InlineKind::CharacterReference(reference) => {
                if self.decode_char_refs {
                    w.write_str(&crate::character_reference_text(reference.kind()))
                } else {
                    write!(w, "{}", reference.kind())
                }
            }
            InlineKind::Raw(text) => {
                let content = if self.projection == TextProjection::Source {
                    Cow::Borrowed(text.text())
                } else {
                    apply_passthrough_replacements_iter(
                        text.text(),
                        text.substitutions(),
                        &Replacements::unicode(),
                        self.text_boundaries,
                    )
                };
                if self.decode_char_refs
                    && text.substitutions().last() != Some(Substitution::SpecialChars)
                {
                    w.write_str(&decode_numeric_char_refs(&content))
                } else {
                    w.write_str(&content)
                }
            }
            InlineKind::Verbatim(text) => w.write_str(text.text()),
            InlineKind::Bold(span) => self.write_content(w, span.content()),
            InlineKind::Italic(span) => self.write_content(w, span.content()),
            InlineKind::Monospace(span) => self.write_content(w, span.content()),
            InlineKind::Highlight(span) => self.write_content(w, span.content()),
            InlineKind::Subscript(span) => self.write_content(w, span.content()),
            InlineKind::Superscript(span) => self.write_content(w, span.content()),
            InlineKind::CurvedQuotation(span) => {
                w.write_char('“')?;
                self.write_content(w, span.content())?;
                w.write_char('”')
            }
            InlineKind::CurvedApostrophe(span) => {
                w.write_char('‘')?;
                self.write_content(w, span.content())?;
                w.write_char('’')
            }
            InlineKind::Symbol(symbol) => w.write_str(symbol.text()),
            InlineKind::StandaloneCurvedApostrophe(_) => w.write_char('\u{2019}'),
            InlineKind::LineBreak(_) => w.write_str(self.line_break),
            InlineKind::Callout(callout) => write!(w, "<{}>", callout.number()),
            InlineKind::Link(link) => self.write_link_text(w, link.text(), &link.target()),
            InlineKind::Url(url) => self.write_link_text(w, url.text(), &url.target()),
            InlineKind::Mailto(mailto) => self.write_link_text(w, mailto.text(), &mailto.target()),
            InlineKind::Autolink(autolink) => write!(w, "{}", autolink.target()),
            InlineKind::CrossReference(xref) => self.write_xref_text(w, xref),
            InlineKind::IndexTerm(term) if term.is_visible() => self.write_content(w, term.term()),
            InlineKind::Pass(pass) => w.write_str(pass.text().unwrap_or_default()),
            InlineKind::Keyboard(keyboard) => {
                for (index, key) in keyboard.keys().enumerate() {
                    if index > 0 {
                        w.write_char('+')?;
                    }
                    self.write_content(w, key)?;
                }
                Ok(())
            }
            InlineKind::Button(button) => self.write_content(w, button.label()),
            InlineKind::Menu(menu) => {
                self.write_content(w, menu.target())?;
                for item in menu.items() {
                    w.write_str(" > ")?;
                    self.write_content(w, item)?;
                }
                Ok(())
            }
            InlineKind::Image(image) => {
                w.write_char('[')?;
                if let Some(alt) = image.alt() {
                    self.write_content(w, alt)?;
                } else {
                    write!(w, "{}", image.source())?;
                }
                w.write_char(']')
            }
            InlineKind::Icon(icon) => {
                write!(
                    w,
                    "[{}]",
                    crate::icon::alt_from_content(icon.target(), icon.alt())
                )
            }
            InlineKind::Footnote(footnote) => write!(w, "[{}]", footnote.number()),
            InlineKind::Stem(stem) => w.write_str(stem.text()),
            InlineKind::Anchor(_) | InlineKind::IndexTerm(_) | _ => Ok(()),
        }
    }

    /// Write what a cross-reference reads as: its own text, else the target's
    /// reference text, else the `[id]` fallback asciidoctor uses.
    fn write_xref_text<W: Write + ?Sized>(
        self,
        w: &mut W,
        xref: CrossReferenceRef<'_>,
    ) -> fmt::Result {
        if !xref.text().is_empty() {
            return self.write_content(w, xref.text());
        }
        let target = xref.target();
        let Some(references) = self.references else {
            return write!(w, "{target}");
        };
        if self.resolving_xref {
            return write!(w, "[{target}]");
        }
        match references.get(target).and_then(reference_text) {
            Some(nodes) => Self {
                resolving_xref: true,
                ..self
            }
            .write_content(w, nodes),
            None => write!(w, "[{}]", stylized_id(target)),
        }
    }

    fn write_link_text<W: Write + ?Sized>(
        self,
        w: &mut W,
        text: ContentRef<'_>,
        target: &impl fmt::Display,
    ) -> fmt::Result {
        if text.is_empty() {
            write!(w, "{target}")
        } else {
            self.write_content(w, text)
        }
    }

    /// Extract plain text from typed content using this extraction policy.
    #[must_use]
    pub fn content_to_string(self, content: ContentRef<'_>) -> String {
        let mut text = String::new();
        // Writing into a `String` is infallible.
        let _ = self.write_content(&mut text, content);
        text
    }

    /// Extract plain text from `inlines` as a `String`.
    #[must_use]
    pub fn to_string(self, inlines: &[InlineNode<'_>]) -> String {
        let mut s = String::new();
        // Writing into a `String` is infallible.
        let _ = self.write(&mut s, inlines);
        s
    }
}

/// The stylized form of a reference target, which is what a reference to an
/// unknown target reads as.
///
/// An inter-document target keeps its fragment but drops the file extension,
/// so `other.adoc#part` reads as `other#part`, matching asciidoctor.
fn stylized_id(target: &str) -> Cow<'_, str> {
    let (path, fragment) = match target.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (target, None),
    };
    let stem = path
        .rsplit_once('.')
        .filter(|(before, _)| !before.is_empty() && !before.ends_with('/'))
        .map_or(path, |(before, _)| before);
    match fragment {
        _ if stem.len() == path.len() => Cow::Borrowed(target),
        None => Cow::Borrowed(stem),
        Some(fragment) => Cow::Owned(format!("{stem}#{fragment}")),
    }
}

/// Extract plain text from `inlines` as a `String`.
#[must_use]
pub fn inlines_to_string(inlines: &[InlineNode<'_>]) -> String {
    InlineTextTransform::default().to_string(inlines)
}

#[cfg(test)]
mod tests {
    use acdc_parser::{Block, InlineNode, LineBreak, Location, Options, Plain, parse};

    #[test]
    fn curved_quotes_remain_text_under_every_projection_policy()
    -> Result<(), Box<dyn std::error::Error>> {
        for mode in [
            acdc_parser::CompatibilityMode::Asciidoctor,
            acdc_parser::CompatibilityMode::Draft,
        ] {
            for (source, expected) in [
                (
                    r#""`double`" and '`single`' and `'."#,
                    "“double” and ‘single’ and ’.",
                ),
                (
                    r#"[#quoted.role]"`*nested*`" and [#single]'`_text_`'."#,
                    "“nested” and ‘text’.",
                ),
                (
                    "[subs=none]\n\"`literal`\" and '`literal`'.",
                    r#""`literal`" and '`literal`'."#,
                ),
            ] {
                let parsed = parse(
                    source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let parsed_blocks: Vec<_> = parsed
                    .document_ref()
                    .blocks()
                    .map(acdc_parser::Block::from)
                    .collect();
                let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
                    return Err("expected one paragraph".into());
                };
                let transform = super::InlineTextTransform::default();
                for policy in [transform, transform.rendered()] {
                    assert_eq!(
                        policy.to_string(&paragraph.content),
                        expected,
                        "{mode:?}: {source}"
                    );
                    assert_eq!(
                        {
                            let block = parsed.document_ref().blocks().next().ok_or("paragraph")?;
                            let acdc_parser::ast::BlockKind::Paragraph(paragraph) = block.kind()
                            else {
                                return Err("expected paragraph".into());
                            };
                            policy.content_to_string(paragraph.content())
                        },
                        expected
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn typed_content_preserves_source_and_rendered_text() -> Result<(), Box<dyn std::error::Error>>
    {
        let parsed = parse(
            "A *bold* image:bird.png[alt='Bird',title='Tooltip'] indexterm2:[--] end.",
            &Options::default(),
        )?;
        let paragraph = parsed.document_ref().blocks().next().ok_or("paragraph")?;
        let acdc_parser::ast::BlockKind::Paragraph(paragraph) = paragraph.kind() else {
            return Err("expected paragraph".into());
        };
        let content = paragraph.content();
        let transform = super::InlineTextTransform::default();
        assert_eq!(
            transform.content_to_string(content),
            "A bold [Bird]\u{2009}—\u{2009}end."
        );
        assert_eq!(
            transform.rendered().content_to_string(content),
            "A bold [Bird]\u{2009}—\u{2009}end.",
        );
        Ok(())
    }

    #[test]
    fn typed_content_stops_at_the_first_writer_error() -> Result<(), Box<dyn std::error::Error>> {
        struct RejectText {
            calls: usize,
        }
        impl std::fmt::Write for RejectText {
            fn write_str(&mut self, _: &str) -> std::fmt::Result {
                self.calls += 1;
                Err(std::fmt::Error)
            }
        }
        let parsed = parse("First *nested* last.", &Options::default())?;
        let paragraph = parsed.document_ref().blocks().next().ok_or("paragraph")?;
        let acdc_parser::ast::BlockKind::Paragraph(paragraph) = paragraph.kind() else {
            return Err("expected paragraph".into());
        };
        let mut writer = RejectText { calls: 0 };
        assert!(
            super::InlineTextTransform::default()
                .write_content(&mut writer, paragraph.content())
                .is_err()
        );
        assert_eq!(writer.calls, 1);
        Ok(())
    }

    #[test]
    fn isolated_index_labels_retain_their_enclosing_text_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        for (source, expected) in [
            ("indexterm2:[--]", "\u{2009}—\u{2009}"),
            ("indexterm2:[--]tail", "--tail"),
            ("prefixindexterm2:[--]", "prefix--"),
            ("prefixindexterm2:[--]tail", "prefix—\u{200b}tail"),
            ("prefixindexterm2:[(C)]tail", "prefix©tail"),
        ] {
            let parsed = parse(source, &Options::default())?;
            let parsed_blocks: Vec<_> = parsed
                .document_ref()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let transform = super::InlineTextTransform::default().rendered();
            assert_eq!(
                transform.to_string(&paragraph.content),
                expected,
                "{source}"
            );
            let mut fragments = String::new();
            for (index, node) in paragraph.content.iter().enumerate() {
                fragments.push_str(
                    &transform
                        .text_boundaries(
                            super::TextBoundaries::BOTH.for_inline(&paragraph.content, index),
                        )
                        .to_string(std::slice::from_ref(node)),
                );
            }
            assert_eq!(fragments, expected, "{source}");
        }
        Ok(())
    }

    #[test]
    fn index_dash_context_preserves_resolved_text_and_neighboring_words()
    -> Result<(), Box<dyn std::error::Error>> {
        for prefix in ["word", "é", "字", "word_"] {
            for suffix in ["word", "é", "字", "_tail"] {
                for gap in [
                    String::new(),
                    "indexterm:[Hidden]".into(),
                    "anchor:gap[]".into(),
                    "indexterm:[Hidden]".repeat(128),
                ] {
                    for label in ["--", "--end", "begin--"] {
                        let input = format!("{prefix}{gap}indexterm2:[{label}]{gap}{suffix}");
                        let parsed = parse(&input, &Options::default())?;
                        let parsed_blocks: Vec<_> = parsed
                            .document_ref()
                            .blocks()
                            .map(acdc_parser::Block::from)
                            .collect();
                        let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
                            return Err("expected one paragraph".into());
                        };
                        let transform = super::InlineTextTransform::default();
                        let source = transform.to_string(&paragraph.content);
                        let expected =
                            format!("{prefix}{label}{suffix}").replace("--", "—\u{200b}");
                        assert_eq!(source, expected, "{input}");
                        assert_eq!(
                            transform.rendered().to_string(&paragraph.content),
                            expected,
                            "{input}"
                        );
                        assert_eq!(transform.to_string(&paragraph.content), source, "{input}");
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn index_dash_context_keeps_nonword_boundaries_and_escapes()
    -> Result<(), Box<dyn std::error::Error>> {
        for (input, expected) in [
            ("indexterm2:[--]", "\u{2009}—\u{2009}"),
            ("indexterm:[Hidden]indexterm2:[--]", "\u{2009}—\u{2009}"),
            ("wordindexterm2:[--]", "word--"),
            ("indexterm2:[--]word", "--word"),
            ("word indexterm2:[--] word", "word\u{2009}—\u{2009}word"),
            ("word.indexterm2:[--]word", "word.--word"),
            ("wordindexterm2:[--].word", "word--.word"),
            (r"wordindexterm2:[\--]word", "word--word"),
            ("wordindexterm2:[---]word", "word---word"),
            ("word-indexterm2:[--]word", "word---word"),
            ("wordindexterm2:[--]-word", "word---word"),
            ("wordindexterm2:[--]**bold**", "word--bold"),
        ] {
            let parsed = parse(input, &Options::default())?;
            let parsed_blocks: Vec<_> = parsed
                .document_ref()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            assert_eq!(
                super::InlineTextTransform::default()
                    .rendered()
                    .to_string(&paragraph.content),
                expected,
                "{input}"
            );
        }
        Ok(())
    }

    #[test]
    fn index_replacement_context_consumes_only_enabled_source_text()
    -> Result<(), Box<dyn std::error::Error>> {
        for (input, expected) in [
            ("prefix indexterm2:[--] tail", "prefix\u{2009}—\u{2009}tail"),
            ("prefix-indexterm2:[-]tail", "prefix—\u{200b}tail"),
            ("prefixindexterm2:[-]-tail", "prefix—\u{200b}tail"),
            (
                "prefixindexterm2:[-]indexterm2:[-]tail",
                "prefix—\u{200b}tail",
            ),
            (
                "prefix-indexterm:[Hidden]indexterm2:[-]tail",
                "prefix—\u{200b}tail",
            ),
            (
                "prefix-indexterm2:[-]anchor:end[]tail",
                "prefix—\u{200b}tail",
            ),
            ("Saindexterm2:[m]'s", "Sam’s"),
            ("indexterm2:[Sam']s", "Sam’s"),
            ("Samindexterm2:[']s", "Sam’s"),
            ("préindexterm2:[']été", "pré’été"),
            ("3indexterm2:[']4", "3'4"),
            ("_indexterm2:[']s", "_'s"),
            (r"Samindexterm2:[\']s", "Sam's"),
            (r"Sam\indexterm2:[']s", "Samindexterm2:[']s"),
            (r"prefix\indexterm2:[-]-tail", "prefixindexterm2:[-]-tail"),
            (r"prefix-\indexterm2:[-]tail", "prefix-indexterm2:[-]tail"),
            (
                "prefix  indexterm2:[--]  tail",
                "prefix \u{2009}—\u{2009} tail",
            ),
            ("prefix-indexterm2:[--]tail", "prefix---tail"),
            ("prefix indexterm2:[pass:[--]] tail", "prefix -- tail"),
            ("prefixpass:[ ]indexterm2:[--] tail", "prefix -- tail"),
            ("prefix indexterm2:[--]pass:[ ]tail", "prefix -- tail"),
            ("prefixpass:[-]indexterm2:[-]tail", "prefix--tail"),
            (
                "prefix indexterm2:[pass:r[--]] tail",
                "prefix \u{2009}—\u{2009} tail",
            ),
            (
                "prefixindexterm2:[pass:r[--]]tail",
                "prefix\u{2009}—\u{2009}tail",
            ),
        ] {
            let parsed = parse(input, &Options::default())?;
            let parsed_blocks: Vec<_> = parsed
                .document_ref()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let source = super::InlineTextTransform::default().to_string(&paragraph.content);
            let rendered = super::InlineTextTransform::default()
                .rendered()
                .to_string(&paragraph.content);
            assert_eq!(rendered, expected, "{input}");
            assert_eq!(
                super::InlineTextTransform::default().to_string(&paragraph.content),
                source
            );
        }
        Ok(())
    }

    #[test]
    fn resolved_text_fragments_keep_literal_text_at_every_split() {
        for input in [
            "a -- b",
            "a--b",
            "-- -- --",
            "a -- -- -- -- -- b",
            "a --  -- b",
            "a---b",
            "a-- --b",
            "Sam's",
            "pré'été",
            r"Sam\'s",
            r"a\--b",
            "a\n--\nb",
            "a --\n--\n-- b",
            "-- -- -- -- -- -- -- -- -- -- --",
        ] {
            let expected = input;
            for split in input.char_indices().map(|(index, _)| index).skip(1) {
                let fragments = [plain(&input[..split]), plain(&input[split..])];
                assert_eq!(
                    super::InlineTextTransform::default()
                        .rendered()
                        .to_string(&fragments),
                    expected,
                    "{input:?} at {split}"
                );
            }
            let characters = input.chars().map(|ch| ch.to_string()).collect::<Vec<_>>();
            let fragments = characters
                .iter()
                .map(|text| plain(text))
                .collect::<Vec<_>>();
            assert_eq!(
                super::InlineTextTransform::default()
                    .rendered()
                    .to_string(&fragments),
                expected,
                "{input:?} split by character"
            );
        }
    }

    #[test]
    fn replacement_context_only_neighbors_cannot_consume_source() {
        let nodes = [plain("word "), plain("--"), plain(" tail")];
        let boundaries = super::TextBoundaries::BOTH
            .for_inline(&nodes, 1)
            .with_neighbors(Some(' '), Some(' '));
        assert_eq!(
            super::Replacements::unicode().transform_verbatim("--", boundaries),
            "--"
        );
        let word_boundaries = boundaries.with_neighbors(Some('d'), Some('t'));
        assert_eq!(
            super::Replacements::unicode().transform_verbatim("--", word_boundaries),
            "—\u{200b}"
        );
    }

    #[test]
    fn index_replacement_context_cannot_disable_resolved_symbols()
    -> Result<(), Box<dyn std::error::Error>> {
        for (input, expected) in [
            ("prefix indexterm2:[--] tail", "prefix\u{2009}—\u{2009}tail"),
            ("prefix-indexterm2:[-]tail", "prefix—\u{200b}tail"),
            ("Saindexterm2:[m]'s", "Sam’s"),
            (
                "prefixindexterm2:[pass:r[--]]tail",
                "prefix\u{2009}—\u{2009}tail",
            ),
            (
                "prefix indexterm2:[pass:r[--]] tail",
                "prefix \u{2009}—\u{2009} tail",
            ),
            ("prefix-indexterm2:[pass:r[-]]tail", "prefix--tail"),
        ] {
            let parsed = parse(input, &Options::default())?;
            let parsed_blocks: Vec<_> = parsed
                .document_ref()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            assert_eq!(
                super::InlineTextTransform::default()
                    .rendered()
                    .to_string(&paragraph.content),
                expected,
                "{input}"
            );
        }
        Ok(())
    }

    #[test]
    fn resolved_code_text_keeps_literal_tokens_and_escapes() {
        let nodes = [plain(r"(C) \(R) \*literal\* \->")];
        let source = super::InlineTextTransform::default();
        assert_eq!(source.to_string(&nodes), r"(C) \(R) \*literal\* \->");
        assert_eq!(
            source.rendered().to_string(&nodes),
            r"(C) \(R) \*literal\* \->"
        );
    }

    #[test]
    fn resolved_typography_preserves_literal_escapes() -> Result<(), Box<dyn std::error::Error>> {
        for source in [
            r"pass:r[(C) \(R) \*literal\* \->]",
            ":value: pass:r[(C) \\(R) \\*literal\\* \\->]\n\n{value}",
        ] {
            let parsed = parse(source, &Options::default())?;
            let parsed_blocks: Vec<_> = parsed
                .document_ref()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let transform = super::InlineTextTransform::default();
            assert_eq!(
                transform.to_string(&paragraph.content),
                r"© (R) \*literal\* \->"
            );
            assert_eq!(
                transform.rendered().to_string(&paragraph.content),
                r"© (R) \*literal\* \->"
            );
            assert_eq!(
                transform.to_string(&paragraph.content),
                r"© (R) \*literal\* \->"
            );
        }
        Ok(())
    }

    #[test]
    fn passthrough_source_arrows_expose_the_resolved_parser_meaning()
    -> Result<(), Box<dyn std::error::Error>> {
        for (profile, expected) in [
            ("c,r", "→ ⇒ ← ⇐"),
            ("r,c", "-> => <- <="),
            ("r", "-> => <- <="),
            ("c", "-> => <- <="),
        ] {
            let source = format!("pass:{profile}[-> => <- <=]");
            let parsed = parse(&source, &Options::default())?;
            let parsed_blocks: Vec<_> = parsed
                .document_ref()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let transform = super::InlineTextTransform::default();
            assert_eq!(transform.to_string(&paragraph.content), expected);
            assert_eq!(
                transform.rendered().to_string(&paragraph.content),
                expected,
                "{profile}",
            );
            assert_eq!(transform.to_string(&paragraph.content), expected);
        }
        Ok(())
    }

    #[test]
    fn encoded_passthrough_arrows_are_resolved_before_rendering()
    -> Result<(), Box<dyn std::error::Error>> {
        for (content, arrow) in [
            ("-&gt;", "→"),
            ("=&gt;", "⇒"),
            ("&lt;-", "←"),
            ("&lt;=", "⇐"),
            (r"\-&gt;", "->"),
            (r"\=&gt;", "=>"),
            (r"\&lt;-", "<-"),
            (r"\&lt;=", "<="),
            ("&lt;-&gt;", "<→"),
            ("&lt;=&gt;", "<⇒"),
            (r"&lt;\-&gt;", "←>"),
            (r"&lt;\=&gt;", "⇐>"),
            (r"\&lt;-&gt;", r"\<→"),
        ] {
            let source = format!("α pass:r[{content}] ω");
            let parsed = parse(&source, &Options::default())?;
            let parsed_blocks: Vec<_> = parsed
                .document_ref()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let transform = super::InlineTextTransform::default();
            assert_eq!(
                transform.to_string(&paragraph.content),
                format!("α {arrow} ω")
            );
            let rendered = transform.rendered().to_string(&paragraph.content);
            assert_eq!(rendered, format!("α {arrow} ω"));
            assert_eq!(
                transform.to_string(&paragraph.content),
                format!("α {arrow} ω")
            );
        }
        Ok(())
    }

    #[test]
    fn deferred_typography_does_not_reapply_surrounding_replacements()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse(r"pass:r[\(C)] (R) pass:[(TM)]", &Options::default())?;
        let parsed_blocks: Vec<_> = parsed
            .document_ref()
            .blocks()
            .map(acdc_parser::Block::from)
            .collect();
        let [Block::Paragraph(paragraph)] = parsed_blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        assert_eq!(
            super::InlineTextTransform::default()
                .rendered()
                .to_string(&paragraph.content),
            "(C) ® (TM)"
        );
        Ok(())
    }

    use super::{InlineTextTransform, inlines_to_string};

    fn plain(content: &str) -> InlineNode<'_> {
        InlineNode::PlainText(Plain {
            content,
            location: Location::default(),
            escaped: false,
        })
    }

    #[test]
    fn inlines_to_string_collapses_line_break_to_space() {
        let inlines = vec![
            plain("first"),
            InlineNode::LineBreak(LineBreak {
                location: Location::default(),
            }),
            plain("second"),
        ];

        assert_eq!(inlines_to_string(&inlines), "first second");
    }

    #[test]
    fn transform_uses_requested_line_break() {
        let inlines = vec![
            plain("first"),
            InlineNode::LineBreak(LineBreak {
                location: Location::default(),
            }),
            plain("second"),
        ];

        assert_eq!(
            InlineTextTransform::default()
                .line_break("\n")
                .to_string(&inlines),
            "first\nsecond"
        );
    }

    /// Parse a document whose body paragraph holds the cross-references under
    /// test, and whose catalog holds `titled`, `labelled`, and `untitled`.
    fn document(body: &str) -> Result<acdc_parser::ParseResult, acdc_parser::Error> {
        let input = format!(
            "= Doc\n:experimental:\n\n{body}\n\n\
             [[titled]]\n.A *title*\n====\nbody\n====\n\n\
             Some [[labelled,A label]]text.\n\n\
             [[untitled]]\n====\nbody\n====\n\n\
             [[recursive]]\n.See <<recursive>> again\n====\nbody\n====\n"
        );
        acdc_parser::parse(&input, &acdc_parser::Options::default())
    }

    /// The inline nodes of the document's first body paragraph.
    fn body_inlines<'a>(doc: &'a acdc_parser::Document<'a>) -> &'a [InlineNode<'a>] {
        doc.blocks
            .iter()
            .find_map(|block| {
                if let acdc_parser::Block::Paragraph(paragraph) = block {
                    Some(paragraph.content.as_slice())
                } else {
                    None
                }
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_cross_reference_without_a_catalog_reads_as_its_target() -> Result<(), acdc_parser::Error> {
        // All an inter-document reference has is its target.
        let parsed = document("See <<titled>>.")?;
        let doc = &acdc_parser::Document::from(parsed.document_ref());
        assert_eq!(inlines_to_string(body_inlines(doc)), "See titled.");
        Ok(())
    }

    #[test]
    fn a_cross_reference_reads_as_its_target_reference_text() -> Result<(), acdc_parser::Error> {
        let parsed = document("See <<titled>>, <<labelled>>, <<untitled>>, and <<missing>>.")?;
        let doc = &acdc_parser::Document::from(parsed.document_ref());
        assert_eq!(
            InlineTextTransform::default()
                .references((&doc.references).into())
                .to_string(body_inlines(doc)),
            "See A title, A label, [untitled], and [missing]."
        );
        Ok(())
    }

    #[test]
    fn named_section_reftext_controls_cross_reference_text() -> Result<(), acdc_parser::Error> {
        let parsed = acdc_parser::parse(
            "Named: <<Custom Label>>. Explicit: <<Custom Label,Chosen text>>. Title: <<Actual Title>>. ID: <<id>>.\n\n[#id,reftext=\"Custom Label\"]\n== Actual Title\n",
            &acdc_parser::Options::default(),
        )?;
        let doc = &acdc_parser::Document::from(parsed.document_ref());

        assert_eq!(
            InlineTextTransform::default()
                .references((&doc.references).into())
                .to_string(body_inlines(doc)),
            "Named: Custom Label. Explicit: Chosen text. Title: [Actual Title]. ID: Custom Label."
        );
        Ok(())
    }

    #[test]
    fn a_cross_reference_inside_reference_text_reads_as_its_id() -> Result<(), acdc_parser::Error> {
        let parsed = document("See <<recursive>>.")?;
        let doc = &acdc_parser::Document::from(parsed.document_ref());
        assert_eq!(
            InlineTextTransform::default()
                .references((&doc.references).into())
                .to_string(body_inlines(doc)),
            "See See [recursive] again."
        );
        Ok(())
    }

    #[test]
    fn a_link_reads_as_its_text_then_its_target() -> Result<(), acdc_parser::Error> {
        let parsed = document("See https://example.com[the site] and https://bare.example[].")?;
        let doc = &acdc_parser::Document::from(parsed.document_ref());
        assert_eq!(
            inlines_to_string(body_inlines(doc)),
            "See the site and https://bare.example."
        );
        Ok(())
    }

    #[test]
    fn a_menu_reads_as_its_target_then_its_items() -> Result<(), acdc_parser::Error> {
        let parsed = document("Choose menu:File[Open > Recent].")?;
        let doc = &acdc_parser::Document::from(parsed.document_ref());
        assert_eq!(
            inlines_to_string(body_inlines(doc)),
            "Choose File > Open > Recent."
        );
        Ok(())
    }

    #[test]
    fn character_references_decode_only_when_asked() -> Result<(), acdc_parser::Error> {
        let parsed = document("It{apos}s here.")?;
        let doc = &acdc_parser::Document::from(parsed.document_ref());
        let inlines = body_inlines(doc);
        assert_eq!(inlines_to_string(inlines), "It&#39;s here.");
        assert_eq!(
            InlineTextTransform::default()
                .decode_char_refs(true)
                .to_string(inlines),
            "It's here."
        );
        Ok(())
    }

    #[test]
    fn an_unknown_target_reads_as_its_stylized_id() {
        assert_eq!(super::stylized_id("plain-id"), "plain-id");
        assert_eq!(super::stylized_id("other.adoc"), "other");
        assert_eq!(super::stylized_id("other.adoc#part"), "other#part");
        assert_eq!(super::stylized_id("dir/other.adoc#part"), "dir/other#part");
        assert_eq!(super::stylized_id("no-extension#part"), "no-extension#part");
    }
}

#[cfg(test)]
mod character_reference_tests {
    use super::InlineTextTransform;
    use acdc_parser::{CharacterReference, CharacterReferenceKind, InlineNode, Location};

    #[test]
    fn character_references_keep_source_identity_and_decode_for_text() -> Result<(), &'static str> {
        for (kind, spelling, text) in [
            (CharacterReferenceKind::Named("amp"), "&amp;", "&"),
            (
                CharacterReferenceKind::Named("NotEqualTilde"),
                "&NotEqualTilde;",
                "≂̸",
            ),
            (
                CharacterReferenceKind::Named("unknown"),
                "&unknown;",
                "&unknown;",
            ),
            (CharacterReferenceKind::Numeric(65), "&#65;", "A"),
            (CharacterReferenceKind::Numeric(0x1f600), "&#128512;", "😀"),
            (
                CharacterReferenceKind::Numeric(0xd800),
                "&#55296;",
                "&#55296;",
            ),
        ] {
            let nodes = [InlineNode::CharacterReference(
                CharacterReference::new(kind, Location::default()).ok_or("invalid reference")?,
            )];
            assert_eq!(InlineTextTransform::default().to_string(&nodes), spelling);
            assert_eq!(
                InlineTextTransform::default()
                    .decode_char_refs(true)
                    .to_string(&nodes),
                text
            );
        }
        Ok(())
    }
}
