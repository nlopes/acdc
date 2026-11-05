use crate::{
    Admonition, AdmonitionForm, Anchor, BlockMetadata, DelimitedBlock, DelimitedBlockType,
    DiscreteHeader, Paragraph, Source, Title,
};

use super::{
    Error, Export, Node, Profile, Target, body_inlines, inlines, metadata, node, pointer,
    unsupported,
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
        self.target(value, title, None, metadata::anchor(value), path)
    }

    pub(super) fn admonition(
        &mut self,
        value: &'a Admonition<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        let mut result = node("admonition", "block");
        match value.form {
            AdmonitionForm::Delimited(delimiter) if !matches!(delimiter, "" | "\"" | ">") => {
                result.insert("form".into(), "delimited".into());
                result.insert("delimiter".into(), delimiter.into());
            }
            AdmonitionForm::Paragraph | AdmonitionForm::Indented
                if self.profile == Profile::Acdc =>
            {
                result.insert(
                    "form".into(),
                    if value.form == AdmonitionForm::Indented {
                        "indented"
                    } else {
                        "paragraph"
                    }
                    .into(),
                );
            }
            AdmonitionForm::Paragraph | AdmonitionForm::Indented => {
                return Err(unsupported(
                    &pointer(path, "form"),
                    "paragraph admonitions need the named acdc profile",
                ));
            }
            AdmonitionForm::Unspecified | AdmonitionForm::Delimited(_) => {
                return Err(unsupported(
                    &pointer(path, "delimiter"),
                    "an admonition needs known source form and an actual delimiter for delimited form",
                ));
            }
        }
        result.insert("variant".into(), value.variant.to_string().into());
        result.insert(
            "blocks".into(),
            self.nonsection_blocks(&value.blocks, &pointer(path, "blocks"))?,
        );
        self.block_metadata(&mut result, &value.metadata, Some(&value.title), path)?;
        Ok(result)
    }

    pub(super) fn paragraph(
        &mut self,
        value: &'a Paragraph<'doc>,
        path: &str,
        literal: bool,
    ) -> Result<Node, Error> {
        let name = match value.metadata.style {
            Some("source" | "listing") => "listing",
            Some("literal") => "literal",
            Some("verse") => "verse",
            Some("pass") => "pass",
            Some(style @ ("sidebar" | "example" | "quote" | "abstract" | "partintro")) => {
                if self.profile == Profile::Upstream {
                    metadata::check(&value.metadata, path, self.profile)?;
                    return Err(unsupported(
                        &pointer(path, "form"),
                        "this paragraph style needs the named acdc profile",
                    ));
                }
                style
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
        let content = body_inlines(
            &value.content,
            &value.metadata,
            path,
            literal || matches!(name, "listing" | "literal" | "verse"),
            self.profile,
        )?;
        result.insert("inlines".into(), content);
        Ok(result)
    }

    pub(super) fn delimited(
        &mut self,
        value: &'a DelimitedBlock<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        let marker = matches!(value.delimiter, "\"" | ">").then_some(value.delimiter);
        if value.delimiter.is_empty()
            || (marker.is_some()
                && (self.profile == Profile::Upstream
                    || !matches!(value.inner, DelimitedBlockType::DelimitedQuote(_))))
        {
            return Err(unsupported(
                &pointer(path, "delimiter"),
                "this source marker is not a delimited block form",
            ));
        }
        let (name, body, content) = match &value.inner {
            DelimitedBlockType::DelimitedListing(content) => (
                "listing",
                "inlines",
                body_inlines(content, &value.metadata, path, true, self.profile)?,
            ),
            DelimitedBlockType::DelimitedLiteral(content) => (
                "literal",
                "inlines",
                body_inlines(content, &value.metadata, path, true, self.profile)?,
            ),
            DelimitedBlockType::DelimitedVerse(content) => (
                "verse",
                "inlines",
                body_inlines(content, &value.metadata, path, true, self.profile)?,
            ),
            DelimitedBlockType::DelimitedPass(content) => (
                "pass",
                "inlines",
                body_inlines(content, &value.metadata, path, false, self.profile)?,
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
            DelimitedBlockType::DelimitedTable(table) => return self.table(value, table, path),
            DelimitedBlockType::DelimitedStem(content) => {
                return self.stem(value, content, path);
            }
            DelimitedBlockType::DelimitedComment(_) => {
                return Err(unsupported(
                    &pointer(path, "inner"),
                    "this block body has no supported upstream mapping",
                ));
            }
        };
        let mut result = node(name, "block");
        if let Some(marker) = marker {
            result.insert("form".into(), "marked".into());
            result.insert("marker".into(), marker.into());
        } else {
            result.insert("form".into(), "delimited".into());
            result.insert("delimiter".into(), value.delimiter.into());
        }
        result.insert(body.into(), content);
        self.block_metadata(&mut result, &value.metadata, Some(&value.title), path)?;
        Ok(result)
    }

    fn stem(
        &mut self,
        block: &'a DelimitedBlock<'doc>,
        content: &crate::StemContent<'_>,
        path: &str,
    ) -> Result<Node, Error> {
        if self.profile == Profile::Upstream {
            return Err(unsupported(
                &pointer(path, "notation"),
                "resolved mathematical notation needs the named acdc profile",
            ));
        }
        let mut result = node("stem", "block");
        result.insert("form".into(), "delimited".into());
        result.insert("delimiter".into(), block.delimiter.into());
        self.block_metadata(&mut result, &block.metadata, Some(&block.title), path)?;
        let escaping = block.metadata.substitutions.as_ref().is_none_or(|spec| {
            spec.contains(
                &crate::Substitution::SpecialChars,
                &[crate::Substitution::SpecialChars],
            )
        });
        super::super::math::apply(&mut result, content.content, &content.notation, escaping);
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
        if self.profile == Profile::Upstream && anchors.len() > 1 {
            return Err(unsupported(
                &pointer(path, "anchors"),
                "multiple item identities need a lossless mapping",
            ));
        }
        super::super::identities::apply(
            result,
            anchors.first(),
            anchors.iter(),
            self.profile,
            path,
        )?;
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
                    inlines(label.as_inlines(), &pointer(path, "reftext"), self.profile)?,
                );
            }
            self.insert_target(
                anchor.id.into(),
                Target {
                    owner: Some(path.into()),
                    title: title.filter(|title| title.is_present()),
                    label: anchor.xreflabel.as_ref(),
                    section: None,
                    caption: None,
                },
                path,
            )?;
        }
        if self.profile == Profile::Acdc {
            for (index, anchor) in anchors.iter().skip(1).enumerate() {
                self.insert_target(
                    anchor.id.into(),
                    Target {
                        owner: Some(pointer(&pointer(path, "anchors"), &index.to_string())),
                        title: title.filter(|title| title.is_present()),
                        label: anchor.xreflabel.as_ref(),
                        section: None,
                        caption: None,
                    },
                    path,
                )?;
            }
        }

        Ok(())
    }
}
