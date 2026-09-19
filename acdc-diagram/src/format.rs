//! Output formats a diagram can be rendered to.

use std::fmt;

use crate::error::Error;

/// An output format a diagram converter can produce.
///
/// The three text formats are rendered as literal blocks rather than images:
/// `txt` is whatever the tool calls plain ASCII art, while `PlantUML`
/// distinguishes `atxt` (pure ASCII) from `utxt` (box-drawing characters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Format {
    /// Portable Network Graphics.
    Png,
    /// Scalable Vector Graphics.
    Svg,
    /// Portable Document Format.
    Pdf,
    /// Graphics Interchange Format.
    Gif,
    /// JPEG.
    Jpeg,
    /// ASCII art, rendered as a literal block.
    Txt,
    /// ASCII-only art, rendered as a literal block.
    Atxt,
    /// Unicode art, rendered as a literal block.
    Utxt,
}

impl Format {
    /// The format's name as it appears in a `format=` attribute and as the
    /// generated file's extension.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Svg => "svg",
            Format::Pdf => "pdf",
            Format::Gif => "gif",
            Format::Jpeg => "jpeg",
            Format::Txt => "txt",
            Format::Atxt => "atxt",
            Format::Utxt => "utxt",
        }
    }

    /// Whether this format becomes a literal block instead of an image.
    #[must_use]
    pub fn is_text(self) -> bool {
        matches!(self, Format::Txt | Format::Atxt | Format::Utxt)
    }

    /// Parse a `format=` attribute value.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownFormat`] for anything that is not an output
    /// format acdc knows about at all — a format the *diagram type* does not
    /// support is reported separately, with the list it does support.
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value.trim().to_ascii_lowercase().as_str() {
            "png" => Ok(Format::Png),
            "svg" => Ok(Format::Svg),
            "pdf" => Ok(Format::Pdf),
            "gif" => Ok(Format::Gif),
            "jpeg" | "jpg" => Ok(Format::Jpeg),
            "txt" | "literal" => Ok(Format::Txt),
            "atxt" => Ok(Format::Atxt),
            "utxt" => Ok(Format::Utxt),
            other => Err(Error::UnknownFormat(other.to_string())),
        }
    }

    /// Render a supported-format list for an error message.
    pub(crate) fn list(formats: &[Format]) -> String {
        formats
            .iter()
            .map(|format| format.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
