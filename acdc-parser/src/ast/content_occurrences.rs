use std::{iter::FusedIterator, slice};

use crate::InlineNode;

use super::{
    AnchorRef, BlockRef, ContentFieldRef, DocumentRef, InlineRef, StructuralRef,
    content_fields::Fields, traversal::Children,
};

/// A lazy depth-first walk of owned content fields.
///
/// Each field precedes the fields owned by its direct inline children. Structural
/// nodes visit their own fields before their children, in [`StructuralRef::children`]
/// order. This is semantic order, not a sort by source position.
///
/// Document footnote, reference and TOC catalogs, index snapshots, and generated
/// bibliography citation text are excluded. The walk neither renders content nor
/// changes registrations. It uses a growable stack proportional to nesting and
/// does not recurse on the call stack.
#[derive(Debug)]
pub struct ContentOccurrences<'doc> {
    stack: Vec<Work<'doc>>,
    pending: Option<ContentFieldRef<'doc>>,
}

impl<'doc> ContentOccurrences<'doc> {
    fn structural(node: StructuralRef<'doc>) -> Self {
        Self {
            stack: vec![Work::Node(node)],
            pending: None,
        }
    }

    fn fields(fields: Fields<'doc>) -> Self {
        Self {
            stack: vec![Work::Fields(fields)],
            pending: None,
        }
    }

    /// Skip fields nested inside the last returned field.
    ///
    /// Sibling fields and structural children are still visited. Call this after
    /// `next()` and before advancing again; repeated calls have no further effect.
    ///
    /// ```
    /// use acdc_parser::{Options, parse, ast::{ContentRole, InlineKind}};
    ///
    /// let parsed = parse("image:bird.png[alt='*Bird*',title='*Tip*']", &Options::default())?;
    /// let mut fields = parsed.document_ref().content_occurrences();
    /// let mut words = Vec::new();
    /// while let Some(field) = fields.next() {
    ///     if field.role() == ContentRole::Tooltip {
    ///         fields.skip_descendants();
    ///         continue;
    ///     }
    ///     for inline in field.content().inlines() {
    ///         if let InlineKind::Plain(text) = inline.kind() {
    ///             words.push(text.text());
    ///         }
    ///     }
    /// }
    /// assert_eq!(words, ["Bird"]);
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    pub fn skip_descendants(&mut self) {
        self.pending = None;
    }
}

#[derive(Debug)]
enum Work<'doc> {
    Node(StructuralRef<'doc>),
    Children(Children<'doc>),
    Fields(Fields<'doc>),
    Inlines(slice::Iter<'doc, InlineNode<'doc>>),
}

impl<'doc> Iterator for ContentOccurrences<'doc> {
    type Item = ContentFieldRef<'doc>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(field) = self.pending.take() {
            self.stack.push(Work::Inlines(field.content().nodes.iter()));
        }
        while let Some(work) = self.stack.pop() {
            match work {
                Work::Node(node) => {
                    self.stack.push(Work::Children(node.child_iter()));
                    self.stack.push(Work::Fields(node.field_iter()));
                }
                Work::Children(mut children) => {
                    if let Some(child) = children.next() {
                        self.stack.push(Work::Children(children));
                        self.stack.push(Work::Node(child));
                    }
                }
                Work::Fields(mut fields) => {
                    if let Some(field) = fields.next() {
                        self.stack.push(Work::Fields(fields));
                        self.pending = Some(field);
                        return Some(field);
                    }
                }
                Work::Inlines(mut inlines) => {
                    if let Some(inline) = inlines.next() {
                        self.stack.push(Work::Inlines(inlines));
                        self.stack
                            .push(Work::Fields(InlineRef::from(inline).field_iter()));
                    }
                }
            }
        }
        None
    }
}

impl FusedIterator for ContentOccurrences<'_> {}

impl<'doc> StructuralRef<'doc> {
    /// Owned fields on this node and its descendants, including nested inlines.
    /// See [`ContentOccurrences`] for order, filtering, and catalog exclusions.
    #[must_use]
    pub fn content_occurrences(self) -> ContentOccurrences<'doc> {
        ContentOccurrences::structural(self)
    }
}

macro_rules! content_roots {
    ($($read:ident),+) => { $(
        impl<'doc> $read<'doc> {
            /// Owned content fields on this node and its descendants.
            /// See [`ContentOccurrences`] for order, filtering, and catalog exclusions.
            #[must_use]
            pub fn content_occurrences(self) -> ContentOccurrences<'doc> {
                StructuralRef::from(self).content_occurrences()
            }
        }
    )+ };
}

content_roots!(DocumentRef, BlockRef);

impl<'doc> InlineRef<'doc> {
    /// Owned fields on this inline and its descendants, excluding this inline itself.
    #[must_use]
    pub fn content_occurrences(self) -> ContentOccurrences<'doc> {
        ContentOccurrences::fields(self.field_iter())
    }
}

impl<'doc> AnchorRef<'doc> {
    /// The retained label and its nested fields, without generated citation copies.
    #[must_use]
    pub fn content_occurrences(self) -> ContentOccurrences<'doc> {
        ContentOccurrences::fields(self.field_iter())
    }
}
