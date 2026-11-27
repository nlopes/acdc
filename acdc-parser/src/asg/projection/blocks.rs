use crate::{
    Admonition, AdmonitionForm, Anchor, BlockMetadata, DiscreteHeading, Image, Paragraph, Source,
    ThematicBreak, Title,
    model::{Delimited, StemContent},
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

    /// Check the delimiter form shared by every delimited kind.
    ///
    /// Returns the quote marker (`"` or `>`) when a quote block uses one.
    pub(super) fn delimited_marker<B>(
        &self,
        value: &'a Delimited<'doc, B>,
        quote: bool,
        path: &str,
    ) -> Result<Option<&'doc str>, Error> {
        let marker = matches!(value.delimiter, "\"" | ">").then_some(value.delimiter);
        if value.delimiter.is_empty()
            || (marker.is_some() && (self.profile == Profile::Upstream || !quote))
        {
            return Err(unsupported(
                &pointer(path, "delimiter"),
                "this source marker is not a delimited block form",
            ));
        }
        Ok(marker)
    }

    /// Project a delimited block whose body is inline content.
    pub(super) fn delimited_inlines(
        &mut self,
        value: &'a Delimited<'doc, Vec<crate::InlineNode<'doc>>>,
        name: &'static str,
        literal: bool,
        path: &str,
    ) -> Result<Node, Error> {
        let marker = self.delimited_marker(value, false, path)?;
        let content = body_inlines(&value.body, &value.metadata, path, literal, self.profile)?;
        self.delimited_node(value, marker, name, "inlines", content, path)
    }

    /// Project a delimited block whose body is child blocks.
    pub(super) fn delimited_blocks(
        &mut self,
        value: &'a Delimited<'doc, Vec<crate::Block<'doc>>>,
        name: &'static str,
        quote: bool,
        path: &str,
    ) -> Result<Node, Error> {
        let marker = self.delimited_marker(value, quote, path)?;
        let content = self.nonsection_blocks(&value.body, &pointer(path, "blocks"))?;
        self.delimited_node(value, marker, name, "blocks", content, path)
    }

    fn delimited_node<B>(
        &mut self,
        value: &'a Delimited<'doc, B>,
        marker: Option<&str>,
        name: &str,
        body: &str,
        content: serde_json::Value,
        path: &str,
    ) -> Result<Node, Error> {
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

    pub(super) fn stem(
        &mut self,
        block: &'a Delimited<'doc, StemContent<'doc>>,
        path: &str,
    ) -> Result<Node, Error> {
        let content = &block.body;
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

    pub(super) fn image_block(
        &mut self,
        image: &'a Image<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        if image.alt.is_some() && self.profile == Profile::Upstream {
            return Err(unsupported(
                &pointer(path, "alt"),
                "parsed alternative content needs an explicit content-attribute mapping",
            ));
        }
        let mut result = self.block_macro(
            "image",
            Some(&image.source),
            Some(&image.title),
            &image.metadata,
            path,
        )?;
        super::super::images::block(&mut result, image, path, self.profile)?;
        Ok(result)
    }

    pub(super) fn heading(
        &mut self,
        value: &'a DiscreteHeading<'doc>,
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

    pub(super) fn thematic_break(
        &mut self,
        value: &'a ThematicBreak<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        if self.profile == Profile::Upstream && value.anchors.len() > 1 {
            return Err(unsupported(
                &pointer(path, "anchors"),
                "multiple break identities need a lossless mapping",
            ));
        }
        let mut result = node("break", "block");
        result.insert("variant".into(), "thematic".into());
        metadata::title(&mut result, &value.title, path, self.profile)?;
        super::super::identities::apply(
            &mut result,
            value.primary_anchor(),
            value.anchors.iter(),
            self.profile,
            path,
        )?;
        if let Some(primary) = value.primary_anchor() {
            if primary.is_bibliography() {
                return Err(unsupported(
                    &pointer(path, "anchors"),
                    "bibliography identity has no supported upstream mapping",
                ));
            }
            if let Some(label) = &primary.xreflabel {
                result.insert(
                    "reftext".into(),
                    inlines(label.as_inlines(), &pointer(path, "reftext"), self.profile)?,
                );
            }
            self.insert_target(
                primary.id.into(),
                Target {
                    owner: Some(path.into()),
                    title: value.title.is_present().then_some(&value.title),
                    label: primary.xreflabel.as_ref(),
                    section: None,
                    caption: None,
                },
                path,
            )?;
        }
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
