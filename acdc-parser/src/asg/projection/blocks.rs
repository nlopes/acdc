use crate::{
    Anchor, BlockMetadata, DelimitedBlock, DelimitedBlockType, DiscreteHeader, Paragraph, Source,
    Title,
};

use super::{
    Error, Export, Node, Target, inlines, literal_inlines, metadata, node, pointer, unsupported,
};

impl<'a, 'doc> Export<'a, 'doc> {
    pub(super) fn block_metadata(
        &mut self,
        result: &mut Node,
        value: &'a BlockMetadata<'doc>,
        title: Option<&'a Title<'doc>>,
        path: &str,
    ) -> Result<(), Error> {
        metadata::apply(self.profile, result, value, path)?;
        if let Some(title) = title {
            metadata::title(result, title, path, self.profile)?;
        }
        self.target(value, title, None, path)
    }

    pub(super) fn paragraph(
        &mut self,
        value: &'a Paragraph<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        let name = match value.metadata.style {
            Some("source" | "listing") => "listing",
            Some("literal") => "literal",
            Some("verse") => "verse",
            Some("pass") => "pass",
            Some("sidebar" | "example" | "quote" | "abstract" | "partintro") => {
                metadata::check(&value.metadata, path, self.profile)?;
                return Err(unsupported(
                    &pointer(path, "form"),
                    "this paragraph style needs an explicit parent-block form",
                ));
            }
            _ => "paragraph",
        };
        let mut result = node(name, "block");
        let indented = value
            .source_text()
            .is_some_and(|source| source.starts_with([' ', '\t']));
        if name != "paragraph" || indented {
            result.insert(
                "form".into(),
                if indented { "indented" } else { "paragraph" }.into(),
            );
        }
        self.block_metadata(&mut result, &value.metadata, Some(&value.title), path)?;
        let content = if matches!(name, "listing" | "literal" | "verse") {
            literal_inlines(&value.content, &pointer(path, "inlines"), self.profile)?
        } else {
            inlines(&value.content, &pointer(path, "inlines"), self.profile)?
        };
        result.insert("inlines".into(), content);
        Ok(result)
    }

    pub(super) fn delimited(
        &mut self,
        value: &'a DelimitedBlock<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        if matches!(value.delimiter, "" | "\"" | ">") {
            return Err(unsupported(
                &pointer(path, "delimiter"),
                "this source marker is not a delimited block form",
            ));
        }
        let (name, body, content) = match &value.inner {
            DelimitedBlockType::DelimitedListing(content) => (
                "listing",
                "inlines",
                literal_inlines(content, &pointer(path, "inlines"), self.profile)?,
            ),
            DelimitedBlockType::DelimitedLiteral(content) => (
                "literal",
                "inlines",
                literal_inlines(content, &pointer(path, "inlines"), self.profile)?,
            ),
            DelimitedBlockType::DelimitedVerse(content) => (
                "verse",
                "inlines",
                literal_inlines(content, &pointer(path, "inlines"), self.profile)?,
            ),
            DelimitedBlockType::DelimitedPass(content) => (
                "pass",
                "inlines",
                inlines(content, &pointer(path, "inlines"), self.profile)?,
            ),
            DelimitedBlockType::DelimitedOpen(content) => (
                "open",
                "blocks",
                self.nonsection_blocks(content, &pointer(path, "blocks"))?,
            ),
            DelimitedBlockType::DelimitedExample(content) => (
                "example",
                "blocks",
                self.nonsection_blocks(content, &pointer(path, "blocks"))?,
            ),
            DelimitedBlockType::DelimitedSidebar(content) => (
                "sidebar",
                "blocks",
                self.nonsection_blocks(content, &pointer(path, "blocks"))?,
            ),
            DelimitedBlockType::DelimitedQuote(content) => (
                "quote",
                "blocks",
                self.nonsection_blocks(content, &pointer(path, "blocks"))?,
            ),
            DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedTable(_)
            | DelimitedBlockType::DelimitedStem(_) => {
                return Err(unsupported(
                    &pointer(path, "inner"),
                    "this block body has no supported upstream mapping",
                ));
            }
        };
        let mut result = node(name, "block");
        result.insert("form".into(), "delimited".into());
        result.insert("delimiter".into(), value.delimiter.into());
        result.insert(body.into(), content);
        self.block_metadata(&mut result, &value.metadata, Some(&value.title), path)?;
        Ok(result)
    }

    pub(super) fn block_macro(
        &mut self,
        name: &str,
        source: Option<&Source<'_>>,
        title: Option<&'a Title<'doc>>,
        value: &'a BlockMetadata<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        let mut result = node(name, "block");
        result.insert("form".into(), "macro".into());
        if let Some(source) = source {
            result.insert("target".into(), super::super::target(source, path)?.into());
        }
        self.block_metadata(&mut result, value, title, path)?;
        Ok(result)
    }

    pub(super) fn heading(
        &mut self,
        value: &'a DiscreteHeader<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        if !value.title.is_present() {
            return Err(unsupported(
                &pointer(path, "title"),
                "the upstream heading requires a present title",
            ));
        }
        let mut result = node("heading", "block");
        result.insert("level".into(), value.level.into());
        self.block_metadata(&mut result, &value.metadata, Some(&value.title), path)?;
        Ok(result)
    }

    pub(super) fn identity(
        &mut self,
        result: &mut Node,
        anchors: &'a [Anchor<'doc>],
        title: Option<&'a Title<'doc>>,
        path: &str,
    ) -> Result<(), Error> {
        if anchors.len() > 1 {
            return Err(unsupported(
                &pointer(path, "anchors"),
                "multiple item identities need a lossless mapping",
            ));
        }
        if let Some(anchor) = anchors.first() {
            if anchor.is_bibliography() {
                return Err(unsupported(
                    &pointer(path, "anchors"),
                    "bibliography identity has no supported upstream mapping",
                ));
            }
            result.insert("id".into(), anchor.id.into());
            if let Some(label) = &anchor.xreflabel {
                result.insert(
                    "reftext".into(),
                    inlines(label, &pointer(path, "reftext"), self.profile)?,
                );
            }
            self.insert_target(
                anchor.id.into(),
                Target {
                    title: title.filter(|title| title.is_present()),
                    label: anchor.xreflabel.as_ref(),
                    section: None,
                    caption: None,
                },
                path,
            )?;
        }

        Ok(())
    }
}
