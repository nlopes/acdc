use std::borrow::Cow;

use acdc_parser::{
    AttributeValue, Block, DocumentAttribute, DocumentAttributeAssignment, DocumentAttributeValue,
    DocumentAttributes,
};
use rustc_hash::FxHashMap;

type Overlay<'doc> = FxHashMap<&'doc str, &'doc DocumentAttributeAssignment<'doc>>;

/// Attributes at the current position of a document traversal.
///
/// Values borrow the document. Independent passes use independent contexts.
///
/// ```
/// use std::convert::Infallible;
/// use acdc_converters_core::{TraversalContext, visitor::Visitor};
/// use acdc_parser::{Options, Paragraph, parse};
///
/// #[derive(Default)]
/// struct Directories<'doc>(Vec<&'doc str>);
///
/// impl<'doc> Visitor<'doc> for Directories<'doc> {
///     type Error = Infallible;
///
///     fn visit_paragraph(
///         &mut self,
///         context: &mut TraversalContext<'doc>,
///         _: &Paragraph<'_>,
///     ) -> Result<(), Self::Error> {
///         if let Some(directory) = context.get("imagesdir").and_then(|value| value.as_str()) {
///             self.0.push(directory);
///         }
///         Ok(())
///     }
/// }
///
/// let parsed = parse(
///     "= Title\n:imagesdir: first\n\nOne.\n\n:imagesdir: second\n\nTwo.\n",
///     &Options::default(),
/// )?;
/// let document = parsed.document();
/// let mut context = TraversalContext::new(&document.attributes);
/// let mut directories = Directories::default();
/// directories.visit_document(&mut context, document)?;
/// assert_eq!(directories.0, ["first", "second"]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct TraversalContext<'doc> {
    header: &'doc DocumentAttributes<'doc>,
    overlay: Overlay<'doc>,
    scopes: Vec<Overlay<'doc>>,
}

impl<'doc> TraversalContext<'doc> {
    /// Visit blocks in source order, applying attribute events before their callbacks.
    ///
    /// # Errors
    ///
    /// Returns the first visitor error.
    pub fn visit_blocks<V: crate::visitor::Visitor<'doc> + ?Sized>(
        &mut self,
        visitor: &mut V,
        blocks: &'doc [Block<'doc>],
    ) -> Result<(), V::Error> {
        for block in blocks {
            self.visit_block(visitor, block)?;
        }
        Ok(())
    }

    /// Dispatch a block, applying an attribute event before its attribute callback.
    ///
    /// # Errors
    ///
    /// Returns errors from the visitor.
    pub fn visit_block<V: crate::visitor::Visitor<'doc> + ?Sized>(
        &mut self,
        visitor: &mut V,
        block: &'doc Block<'doc>,
    ) -> Result<(), V::Error> {
        use acdc_parser::Block;
        visitor.before_block(self, block)?;
        match block {
            Block::DocumentAttribute(event) => {
                self.apply(event);
                visitor.visit_document_attribute(self, event)
            }
            Block::Section(value) => visitor.visit_section(self, value),
            Block::Paragraph(value) => visitor.visit_paragraph(self, value),
            Block::DelimitedBlock(value) => visitor.visit_delimited_block(self, value),
            Block::OrderedList(value) => visitor.visit_ordered_list(self, value),
            Block::UnorderedList(value) => visitor.visit_unordered_list(self, value),
            Block::DescriptionList(value) => visitor.visit_description_list(self, value),
            Block::CalloutList(value) => visitor.visit_callout_list(self, value),
            Block::Admonition(value) => visitor.visit_admonition(self, value),
            Block::Image(value) => visitor.visit_image(self, value),
            Block::Video(value) => visitor.visit_video(self, value),
            Block::Audio(value) => visitor.visit_audio(self, value),
            Block::ThematicBreak(value) => visitor.visit_thematic_break(self, value),
            Block::PageBreak(value) => visitor.visit_page_break(self, value),
            Block::TableOfContents(value) => visitor.visit_table_of_contents(self, value),
            Block::DiscreteHeader(value) => visitor.visit_discrete_header(self, value),
            Block::Comment(_) => Ok(()),
            _ => visitor.visit_unhandled_block(self, block),
        }
    }

    /// Run a nested document traversal and restore the parent attributes on return.
    ///
    /// Restoration also occurs when the operation returns an error.
    pub fn with_scope<R>(&mut self, operation: impl FnOnce(&mut Self) -> R) -> R {
        self.enter_scope();
        let result = operation(self);
        self.exit_scope();
        result
    }

    /// Initialize an `AsciiDoc` cell's attributes and restore the parent on return.
    pub fn with_table_cell<R>(
        &mut self,
        column: &'doc acdc_parser::TableColumn<'doc>,
        operation: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_scope(|context| {
            for (name, assignment) in column.initial_attributes() {
                context.overlay.insert(name, assignment);
            }
            operation(context)
        })
    }

    /// Start at the end of the document header.
    #[must_use]
    pub fn new(header: &'doc DocumentAttributes<'doc>) -> Self {
        Self {
            header,
            overlay: Overlay::default(),
            scopes: Vec::new(),
        }
    }

    fn apply(&mut self, event: &'doc DocumentAttribute<'doc>) {
        self.overlay.insert(event.name.as_ref(), event.assignment());
    }

    fn enter_scope(&mut self) {
        self.scopes.push(self.overlay.clone());
    }

    fn exit_scope(&mut self) {
        if let Some(overlay) = self.scopes.pop() {
            self.overlay = overlay;
        }
    }

    /// Borrow the immutable header attributes, independent of traversal position.
    #[must_use]
    pub const fn header(&self) -> &'doc DocumentAttributes<'doc> {
        self.header
    }

    /// Whether this traversal is inside a nested document scope.
    #[must_use]
    pub fn is_nested_document(&self) -> bool {
        !self.scopes.is_empty()
    }

    /// Borrow an effective value at this position.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'doc DocumentAttributeValue<'doc>> {
        if let Some(assignment) = self.overlay.get(name) {
            return assignment.value();
        }
        self.header.get(name)
    }

    /// Borrow an explicit assignment, including an unset that hides the header.
    #[must_use]
    pub fn attribute_assignment(
        &self,
        name: &str,
    ) -> Option<&'doc DocumentAttributeAssignment<'doc>> {
        self.overlay
            .get(name)
            .copied()
            .or_else(|| self.header.assignment(name))
    }

    /// Whether an effective value exists at this position.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Whether the document explicitly sets or unsets this attribute.
    #[must_use]
    pub fn is_explicit(&self, name: &str) -> bool {
        self.attribute_assignment(name).is_some()
    }

    /// Expand known references, borrowing unchanged input.
    #[must_use]
    pub fn substitute_attributes<'text>(&self, text: &'text str) -> Cow<'text, str> {
        acdc_parser::substitute_attributes(text, |name| self.get(name))
    }

    /// Copy the current assignments for a separate parsing operation.
    ///
    /// This allocates; ordinary conversion reads borrow the document instead.
    ///
    /// # Errors
    ///
    /// Returns an error if an effective assignment fails configuration validation.
    pub fn to_document_attributes(
        &self,
    ) -> Result<DocumentAttributes<'static>, acdc_parser::Error> {
        let mut inputs: FxHashMap<_, _> = self.header.to_static().into_inputs().collect();
        for (name, assignment) in &self.overlay {
            let value = match AttributeValue::from(*assignment) {
                AttributeValue::String(text) => {
                    AttributeValue::String(Cow::Owned(text.into_owned()))
                }
                AttributeValue::Bool(value) => AttributeValue::Bool(value),
                AttributeValue::None | _ => AttributeValue::None,
            };
            inputs.insert(Cow::Owned((*name).to_owned()), value);
        }
        acdc_parser::Options::builder()
            .with_attributes(inputs)
            .build()
            .map(acdc_parser::Options::into_document_attributes)
            .map(DocumentAttributes::into_static)
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, fmt, ptr::eq};

    use acdc_parser::{Options, parse};

    use super::*;
    use crate::{document_attribute_text, visitor::Visitor};

    type Error = Box<dyn std::error::Error>;

    #[test]
    fn attribute_reads_preserve_values_spelling_and_borrowing() -> Result<(), Error> {
        let options = Options::builder()
            .with_attribute("max-include-depth", "03")
            .build()?;
        let parsed = parse(
            "= Header\n:present:\n:name: \"quoted\"\n:removed!:\n\nBody.\n",
            &options,
        )?;
        let header = &parsed.document().attributes;
        let context = TraversalContext::new(header);
        let depth = context.get("max-include-depth").ok_or("missing depth")?;
        assert_eq!(depth.as_integer(), Some(3));
        assert_eq!(depth.text(), Some("03"));
        assert!(eq(
            depth,
            header
                .get("max-include-depth")
                .ok_or("missing header depth")?
        ));
        assert!(
            context
                .get("present")
                .is_some_and(DocumentAttributeValue::is_presence)
        );
        assert_eq!(document_attribute_text(context.get("present")), Some(""));
        assert_eq!(document_attribute_text(context.get("name")), Some("quoted"));
        assert_eq!(document_attribute_text(Some(depth)), None);
        assert!(context.is_explicit("removed"));
        assert!(!context.contains_key("removed"));
        let mut output = String::from("prefix:");
        depth.write_text(&mut output)?;
        context
            .get("present")
            .ok_or("missing presence")?
            .write_text(&mut output)?;
        assert_eq!(output, "prefix:03");
        let unchanged = "Unicode λ {missing} {removed} {unclosed";
        assert!(
            matches!(context.substitute_attributes(unchanged), Cow::Borrowed(value) if eq(value, unchanged))
        );
        assert_eq!(
            context.substitute_attributes("λ {max-include-depth}/{present}/{name}/{missing}"),
            "λ 03//\"quoted\"/{missing}"
        );
        let defaults = DocumentAttributes::default();
        let defaults = TraversalContext::new(&defaults);
        assert_eq!(
            defaults
                .get("max-include-depth")
                .and_then(DocumentAttributeValue::text),
            None
        );
        assert!(!defaults.is_explicit("max-include-depth"));
        assert_eq!(defaults.substitute_attributes("{max-include-depth}"), "64");
        Ok(())
    }

    #[test]
    fn substitution_keeps_replacement_references_literal() -> Result<(), Error> {
        let options = Options::with_attributes([("name", "{other}"), ("other", "expanded")])?;
        let context = TraversalContext::new(options.document_attributes());
        assert_eq!(context.substitute_attributes("{name}"), "{other}");
        Ok(())
    }

    #[test]
    fn attribute_reads_propagate_output_errors() -> Result<(), Error> {
        struct RejectingWriter;
        impl fmt::Write for RejectingWriter {
            fn write_str(&mut self, _: &str) -> fmt::Result {
                Err(fmt::Error)
            }
        }
        let options = Options::with_attributes([("name", "value")])?;
        let context = TraversalContext::new(options.document_attributes());
        assert_eq!(
            context
                .get("name")
                .ok_or("missing value")?
                .write_text(&mut RejectingWriter),
            Err(fmt::Error)
        );
        Ok(())
    }

    #[derive(Default)]
    struct Observer {
        values: Vec<Option<String>>,
    }

    impl<'doc> Visitor<'doc> for Observer {
        type Error = Infallible;

        fn visit_document_attribute(
            &mut self,
            context: &mut TraversalContext<'doc>,
            event: &'doc DocumentAttribute<'doc>,
        ) -> Result<(), Self::Error> {
            assert!(
                context
                    .attribute_assignment(&event.name)
                    .is_some_and(|applied| eq(applied, event.assignment()))
            );
            self.values.push(
                context
                    .get("name")
                    .and_then(DocumentAttributeValue::text)
                    .map(str::to_owned),
            );
            Ok(())
        }
    }

    #[test]
    fn dispatch_applies_assignments_before_callbacks() -> Result<(), Error> {
        let parsed = parse(
            "= Header\n:name: header\n\nBody.\n\n:name: first\n:name: second\n:name!:\n:name: false\n",
            &Options::default(),
        )?;
        let document = parsed.document();
        let mut context = TraversalContext::new(&document.attributes);
        let mut observer = Observer::default();
        context.visit_blocks(&mut observer, &document.blocks)?;
        assert_eq!(
            observer.values,
            [
                Some("first".into()),
                Some("second".into()),
                None,
                Some("false".into())
            ]
        );
        assert_eq!(
            context
                .header()
                .get("name")
                .and_then(DocumentAttributeValue::text),
            Some("header")
        );
        Ok(())
    }

    #[test]
    fn nested_scope_restores_parent_after_success_and_error() -> Result<(), Error> {
        let parent = parse(
            "= Header\n:name: header\n\nBody.\n\n:name: parent\n",
            &Options::default(),
        )?;
        let child = parse("Body.\n\n:name: child\n", &Options::default())?;
        let mut context = TraversalContext::new(&parent.document().attributes);
        let mut observer = Observer::default();
        context.visit_blocks(&mut observer, &parent.document().blocks)?;
        for fail in [false, true] {
            let result = context.with_scope(|context| {
                let Ok(()) = context.visit_blocks(&mut observer, &child.document().blocks);
                assert!(context.is_nested_document());
                assert_eq!(context.substitute_attributes("{name}"), "child");
                if fail {
                    Err("conversion failed")
                } else {
                    Ok(())
                }
            });
            assert_eq!(result.is_err(), fail);
            assert!(!context.is_nested_document());
            assert_eq!(context.substitute_attributes("{name}"), "parent");
        }
        Ok(())
    }

    #[test]
    fn owned_reparse_boundary_preserves_spelling_unsets_and_implicit_defaults() -> Result<(), Error>
    {
        let options = Options::builder()
            .with_attribute("max-include-depth", "03")
            .build()?;
        let parsed = parse(
            "= Header\n:name: header\n\nBody.\n\n:name: body\n:figure-caption!:\n",
            &options,
        )?;
        let mut context = TraversalContext::new(&parsed.document().attributes);
        let mut observer = Observer::default();
        context.visit_blocks(&mut observer, &parsed.document().blocks)?;
        let snapshot = context.to_document_attributes()?;
        assert_eq!(
            snapshot.get("name").and_then(DocumentAttributeValue::text),
            Some("body")
        );
        assert_eq!(
            snapshot
                .get("max-include-depth")
                .and_then(DocumentAttributeValue::text),
            Some("03")
        );
        assert!(matches!(
            snapshot.assignment("figure-caption"),
            Some(DocumentAttributeAssignment::Unset)
        ));
        assert!(!snapshot.is_explicit("table-caption"));
        Ok(())
    }

    #[test]
    fn table_cell_scope_initializes_attributes_and_restores_parent_on_error() -> Result<(), Error> {
        let parsed = parse(
            "= T\n:doctype: book\n:toc: left\n\n[cols=a]\n|===\n|\nCell.\n|===\n",
            &Options::default(),
        )?;
        let Block::DelimitedBlock(block) = parsed
            .document()
            .blocks
            .first()
            .ok_or("missing table block")?
        else {
            return Err("missing table block".into());
        };
        let acdc_parser::DelimitedBlockType::DelimitedTable(table) = &block.inner else {
            return Err("missing table".into());
        };
        let mut context = TraversalContext::new(&parsed.document().attributes);
        for fail in [false, true] {
            let result = context.with_table_cell(
                table
                    .rows
                    .first()
                    .and_then(|row| row.columns.first())
                    .ok_or("missing table cell")?,
                |context| {
                    assert_eq!(
                        context
                            .get("doctype")
                            .and_then(DocumentAttributeValue::as_str),
                        Some("article")
                    );
                    assert!(!context.contains_key("toc"));
                    if fail {
                        Err("conversion failed")
                    } else {
                        Ok(())
                    }
                },
            );
            assert_eq!(result.is_err(), fail);
            assert_eq!(
                context
                    .get("doctype")
                    .and_then(DocumentAttributeValue::as_str),
                Some("book")
            );
            assert!(
                context
                    .get("toc")
                    .is_some_and(DocumentAttributeValue::is_presence)
            );
            assert_eq!(
                context
                    .get("toc-position")
                    .and_then(DocumentAttributeValue::as_str),
                Some("left")
            );
            assert!(!context.is_nested_document());
        }
        Ok(())
    }
}
