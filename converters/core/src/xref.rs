//! Automatic cross-reference display-text resolution.

use std::{cell::Cell, rc::Rc};

use acdc_parser::{InlineNode, Reference};

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
    /// An explicit reference label (`[[id,label]]`), to render through the
    /// converter's inline pipeline as written. Carries the same scope as
    /// [`XrefDisplay::Title`].
    Label(&'r [InlineNode<'a>], XrefScope<'r>),
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
/// Explicit reference labels take precedence over target titles. Unknown local
/// and untitled targets fall back to `[id]`, matching Asciidoctor, and so does
/// a reference that `guard` reports as nested inside another one's text.
/// Inter-document targets are returned separately for backend-specific links.
#[must_use]
pub fn resolve_xref<'r, 'a>(
    reference: Option<&'r Reference<'a>>,
    target: &str,
    guard: &'r XrefGuard,
) -> XrefDisplay<'r, 'a> {
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
    } else if let Some(title) = &reference.title {
        XrefDisplay::Title(title.as_ref(), guard.enter())
    } else {
        XrefDisplay::Fallback(format!("[{target}]"))
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
    use acdc_parser::{Error, Options, ParseResult, parse};

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

    #[test]
    fn label_takes_precedence_over_title() -> Result<(), Error> {
        // `[[labelled,A label]]` sits on an inline anchor, which has no title;
        // a section with both is covered by the html and terminal fixtures.
        let parsed = catalog()?;
        let references = &parsed.document().references;
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(references.get("labelled"), "labelled", &guard),
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
            resolve_xref(references.get("titled"), "titled", &guard),
            XrefDisplay::Title(..)
        ));
        Ok(())
    }

    #[test]
    fn untitled_target_falls_back_to_its_id() -> Result<(), Error> {
        let parsed = catalog()?;
        let references = &parsed.document().references;
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(references.get("untitled"), "untitled", &guard),
            XrefDisplay::Fallback(text) if text == "[untitled]"
        ));
        Ok(())
    }

    #[test]
    fn absent_target_is_unresolved() {
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(None, "no-such-id", &guard),
            XrefDisplay::Unresolved(text) if text == "[no-such-id]"
        ));
    }

    #[test]
    fn interdocument_target_is_not_an_unresolved_local_id() {
        let guard = XrefGuard::default();
        assert!(matches!(
            resolve_xref(None, "other.adoc#part", &guard),
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

        let display = resolve_xref(references.get("titled"), "titled", &guard);
        assert!(matches!(display, XrefDisplay::Title(..)));
        assert!(guard.is_resolving());
        // While the outer resolution is open, a nested one cannot recurse.
        assert!(matches!(
            resolve_xref(references.get("labelled"), "labelled", &guard),
            XrefDisplay::Nested(text) if text == "[labelled]"
        ));

        drop(display);
        assert!(!guard.is_resolving());
        assert!(matches!(
            resolve_xref(references.get("labelled"), "labelled", &guard),
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

        let display = resolve_xref(references.get("titled"), "titled", &guard);
        assert!(clone.is_resolving());
        drop(display);
        assert!(!clone.is_resolving());
        Ok(())
    }
}
