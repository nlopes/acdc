use std::io::Write;

use acdc_converters_core::{TraversalContext, media::resolve_target};
#[cfg(feature = "images")]
use acdc_parser::ast::SourceKind;
use acdc_parser::{CaptionKind, ast::ImageRef};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor, inlines};

impl<'a, W: Write> TerminalVisitor<'a, '_, W> {
    pub(crate) fn render_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        image: ImageRef<'_>,
    ) -> Result<(), Error> {
        let alt = acdc_converters_core::media::image_alt_from_content(image.source(), image.alt());
        let link = super::terminal_visitor::attribute_text(image.attributes(), "link");

        #[cfg(feature = "images")]
        let rendered_protocol_image = if let SourceKind::Path(path) = image.source().kind() {
            let config = image_config(image, self.processor.terminal_width);
            let path = resolve_target(&path.to_string_lossy(), traversal);
            self.writer_mut().flush()?;
            let displayed = viuer::print_from_file(&path, &config).is_ok();
            if !displayed {
                self.diagnostics.warn_with_advice(
                    format!("failed to display image `{path}`"),
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
            let rendered_alt = match image.alt() {
                Some(content) => inlines::render_content_with_styles_to_owned(
                    content,
                    self.processor,
                    traversal,
                )?,
                None => alt.clone(),
            };
            let text = format!("[Image: {rendered_alt}]");
            if let Some(target) = link {
                let processor = self.processor;
                inlines::maybe_render_osc8_link(target, &text, self.writer_mut(), processor)?;
            } else {
                self.writer_mut().queue(PrintStyledContent(text.italic()))?;
                let source = resolve_target(&image.source().to_string(), traversal);
                if source != alt {
                    self.writer_mut()
                        .queue(PrintStyledContent(format!(" ({source})").dim()))?;
                }
            }
            writeln!(self.writer_mut())?;
        }

        self.render_captioned_title_with_wrapper(
            traversal,
            image.title(),
            image.attributes(),
            Some(CaptionKind::Figure),
            "  ",
            "\n",
        )
    }
}

#[cfg(feature = "images")]
fn image_config(image: ImageRef<'_>, terminal_width: usize) -> viuer::Config {
    let width = dimension(image, "width");
    let height = dimension(image, "height");
    let alignment = super::terminal_visitor::attribute_text(image.attributes(), "float")
        .or_else(|| super::terminal_visitor::attribute_text(image.attributes(), "align"));
    let x = match (alignment, width) {
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
fn dimension(image: ImageRef<'_>, name: &str) -> Option<u32> {
    super::terminal_visitor::attribute_text(image.attributes(), name)
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
}

#[cfg(all(test, feature = "images"))]
mod tests {
    use std::borrow::Cow;

    use acdc_parser::{AttributeValue, BlockMetadata, ElementAttributes, Image, Location, Source};

    use super::*;

    fn image(
        attributes: &[(&'static str, &'static str)],
    ) -> Result<Image<'static>, Box<dyn std::error::Error>> {
        let mut values = ElementAttributes::default();
        for (name, value) in attributes {
            values.set(
                Cow::Borrowed(*name),
                AttributeValue::String(Cow::Borrowed(*value)),
            );
        }
        Ok(Image::new(Source::Name("image.png"), Location::default())
            .with_metadata(BlockMetadata::new().with_attributes(values)?)?)
    }

    #[test]
    fn protocol_image_uses_integer_dimensions_and_alignment()
    -> Result<(), Box<dyn std::error::Error>> {
        let image = image(&[("width", "20"), ("height", "10"), ("align", "right")])?;
        let config = image_config((&image).into(), 80);

        assert_eq!(config.width, Some(20));
        assert_eq!(config.height, Some(10));
        assert_eq!(config.x, 60);
        Ok(())
    }

    #[test]
    fn protocol_image_ignores_percentage_dimensions() -> Result<(), Box<dyn std::error::Error>> {
        let image = image(&[("width", "50%"), ("align", "center")])?;
        let config = image_config((&image).into(), 80);

        assert_eq!(config.width, None);
        assert_eq!(config.x, 0);
        Ok(())
    }
}
