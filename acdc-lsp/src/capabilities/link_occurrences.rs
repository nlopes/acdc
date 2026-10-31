use std::ops::ControlFlow;

use acdc_parser::ast::{DocumentRef, HyperlinkRef, ImageRef};

use super::body_occurrences::{BodyOccurrence, visit_body};

pub(super) enum LinkOccurrence<'doc> {
    Hyperlink(HyperlinkRef<'doc>),
    Image(ImageRef<'doc>),
}

/// Visit navigation links in block bodies, excluding titles, table cells, and callouts.
pub(super) fn visit_links<'doc, B>(
    document: DocumentRef<'doc>,
    mut visit: impl FnMut(LinkOccurrence<'doc>) -> ControlFlow<B>,
) -> ControlFlow<B> {
    visit_body(document, |node| match node {
        BodyOccurrence::Inline(inline) => {
            if let Some(link) = inline.as_hyperlink() {
                visit(LinkOccurrence::Hyperlink(link))?;
            }
            ControlFlow::Continue(())
        }
        BodyOccurrence::Image(image) => visit(LinkOccurrence::Image(image)),
        BodyOccurrence::Section(_) => ControlFlow::Continue(()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_lookup_stops_before_later_links() -> Result<(), acdc_parser::Error> {
        let parsed = acdc_parser::parse(
            "* *link:first.adoc[First]*\n+\nlink:second.adoc[Second]\n",
            &acdc_parser::Options::default(),
        )?;
        let mut visited = 0;
        let result = visit_links(parsed.document_ref(), |link| {
            visited += 1;
            ControlFlow::Break(match link {
                LinkOccurrence::Hyperlink(link) => Some(link.target().to_string()),
                LinkOccurrence::Image(_) => None,
            })
        });
        assert_eq!(
            result.break_value().flatten().as_deref(),
            Some("first.adoc")
        );
        assert_eq!(visited, 1);
        Ok(())
    }
}
