use std::io::Write;

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{CaptionKind, Image};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor, inlines};

impl<W: Write> TerminalVisitor<'_, '_, W> {
    pub(crate) fn render_image(&mut self, image: &Image) -> Result<(), Error> {
        let alt = inlines::block_image_alt(image);
        let link = image.metadata.attributes.get_string("link");

        #[cfg(feature = "images")]
        let rendered_protocol_image = if let acdc_parser::Source::Path(path) = &image.source {
            let config = image_config(image, self.processor.terminal_width);
            self.writer_mut().flush()?;
            let displayed = viuer::print_from_file(path, &config).is_ok();
            if !displayed {
                self.diagnostics.warn_with_advice(
                    format!("failed to display image `{}`", path.display()),
                    "Verify the image path is relative to the input document and that the terminal image feature can load it.",
                );
            }
            self.writer_mut().flush()?;
            displayed
        } else {
            false
        };
        #[cfg(not(feature = "images"))]
        let rendered_protocol_image = false;

        if !rendered_protocol_image || link.is_some() {
            let text = format!("[Image: {alt}]");
            if let Some(target) = link {
                let processor = self.processor.clone();
                inlines::maybe_render_osc8_link(&target, &text, self.writer_mut(), &processor)?;
            } else {
                self.writer_mut().queue(PrintStyledContent(text.italic()))?;
                let source = image.source.to_string();
                if source != alt {
                    self.writer_mut()
                        .queue(PrintStyledContent(format!(" ({source})").dim()))?;
                }
            }
            writeln!(self.writer_mut())?;
        }

        self.render_captioned_title_with_wrapper(
            &image.title,
            &image.metadata,
            Some(CaptionKind::Figure),
            "  ",
            "\n",
        )
    }
}

#[cfg(feature = "images")]
fn image_config(image: &Image<'_>, terminal_width: usize) -> viuer::Config {
    let width = dimension(image, "width");
    let height = dimension(image, "height");
    let alignment = image
        .metadata
        .attributes
        .get_string("float")
        .or_else(|| image.metadata.attributes.get_string("align"));
    let x = match (alignment.as_deref(), width) {
        (Some("center"), Some(width)) => terminal_width.saturating_sub(width as usize) / 2,
        (Some("right"), Some(width)) => terminal_width.saturating_sub(width as usize),
        _ => 0,
    };
    viuer::Config {
        width,
        height,
        x: u16::try_from(x).unwrap_or(u16::MAX),
        ..viuer::Config::default()
    }
}

#[cfg(feature = "images")]
fn dimension(image: &Image<'_>, name: &str) -> Option<u32> {
    image
        .metadata
        .attributes
        .get_string(name)
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
}

#[cfg(all(test, feature = "images"))]
mod tests {
    use std::borrow::Cow;

    use acdc_parser::{AttributeValue, BlockMetadata, ElementAttributes, Location, Source};

    use super::*;

    fn image(attributes: &[(&'static str, &'static str)]) -> Image<'static> {
        let mut values = ElementAttributes::default();
        for (name, value) in attributes {
            values.set(
                Cow::Borrowed(*name),
                AttributeValue::String(Cow::Borrowed(*value)),
            );
        }
        Image::new(Source::Name("image.png"), Location::default())
            .with_metadata(BlockMetadata::new().with_attributes(values))
    }

    #[test]
    fn protocol_image_uses_integer_dimensions_and_alignment() {
        let image = image(&[("width", "20"), ("height", "10"), ("align", "right")]);
        let config = image_config(&image, 80);

        assert_eq!(config.width, Some(20));
        assert_eq!(config.height, Some(10));
        assert_eq!(config.x, 60);
    }

    #[test]
    fn protocol_image_ignores_percentage_dimensions() {
        let image = image(&[("width", "50%"), ("align", "center")]);
        let config = image_config(&image, 80);

        assert_eq!(config.width, None);
        assert_eq!(config.x, 0);
    }
}
