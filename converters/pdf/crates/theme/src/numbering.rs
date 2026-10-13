use std::num::NonZeroUsize;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Selects the page where Arabic page numbering starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageNumberingStart {
    /// Start at the front cover, or at the title page when no cover exists.
    Cover,
    /// Start at the title page, or at the table of contents when no title page exists.
    Title,
    /// Start at the table of contents.
    Toc,
    /// Start on the first page after the table of contents.
    AfterToc,
    /// Start on the first body page.
    #[default]
    Body,
    /// Start on this one-based page within the body.
    BodyPage(NonZeroUsize),
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum NamedPageNumberingStart {
    Cover,
    Title,
    Toc,
    AfterToc,
    Body,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PageNumberingStartValue {
    Named(NamedPageNumberingStart),
    BodyPage(NonZeroUsize),
}

impl<'de> Deserialize<'de> for PageNumberingStart {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match PageNumberingStartValue::deserialize(deserializer)? {
            PageNumberingStartValue::Named(NamedPageNumberingStart::Cover) => Self::Cover,
            PageNumberingStartValue::Named(NamedPageNumberingStart::Title) => Self::Title,
            PageNumberingStartValue::Named(NamedPageNumberingStart::Toc) => Self::Toc,
            PageNumberingStartValue::Named(NamedPageNumberingStart::AfterToc) => Self::AfterToc,
            PageNumberingStartValue::Named(NamedPageNumberingStart::Body) => Self::Body,
            PageNumberingStartValue::BodyPage(page) => Self::BodyPage(page),
        })
    }
}

impl Serialize for PageNumberingStart {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Cover => serializer.serialize_str("cover"),
            Self::Title => serializer.serialize_str("title"),
            Self::Toc => serializer.serialize_str("toc"),
            Self::AfterToc => serializer.serialize_str("after-toc"),
            Self::Body => serializer.serialize_str("body"),
            Self::BodyPage(page) => page.serialize(serializer),
        }
    }
}
