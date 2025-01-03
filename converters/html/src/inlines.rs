use std::io::Write;

use acdc_parser::{
    AttributeValue, Image, ImageSource, InlineMacro, InlineNode, Link, LinkTarget, Pass,
};

use crate::{Processor, Render, RenderOptions};

impl Render for InlineNode {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        match self {
            InlineNode::PlainText(p) => {
                let text = if options.inlines_substitutions {
                    substitution_text(&p.content)
                } else {
                    p.content.clone()
                };
                write!(w, "{text}")?;
            }
            InlineNode::BoldText(b) => {
                if !options.inlines_basic {
                    write!(w, "<strong>")?;
                }
                for inline in &b.content {
                    inline.render(w, processor, options)?;
                }
                if !options.inlines_basic {
                    write!(w, "</strong>")?;
                }
            }
            InlineNode::ItalicText(i) => {
                if !options.inlines_basic {
                    write!(w, "<em>")?;
                }
                for inline in &i.content {
                    inline.render(w, processor, options)?;
                }
                if !options.inlines_basic {
                    write!(w, "</em>")?;
                }
            }
            InlineNode::MonospaceText(m) => {
                if !options.inlines_basic {
                    write!(w, "<code>")?;
                }
                for inline in &m.content {
                    inline.render(w, processor, options)?;
                }
                if !options.inlines_basic {
                    write!(w, "</code>")?;
                }
            }
            InlineNode::Macro(m) => m.render(w, processor, options)?,
            unknown => todo!("inlines: {:?}", unknown),
        };
        Ok(())
    }
}

impl Render for InlineMacro {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        match self {
            InlineMacro::Link(l) => l.render(w, processor, options),
            InlineMacro::Image(i) => i.render(w, processor, options),
            InlineMacro::Pass(p) => p.render(w, processor, options),
            unknown => todo!("inline macro: {:?}", unknown),
        }
    }
}

impl Render for Link {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        let target = match self.target {
            LinkTarget::Url(ref url) => url.to_string(),
            LinkTarget::Path(ref path) => path.to_string_lossy().to_string(),
        };
        let text = self
            .attributes
            .iter()
            .find_map(|(k, v)| {
                // Link macros can only have one positional attribute, which is the text.
                if *v == AttributeValue::None {
                    Some(k)
                } else {
                    None
                }
            })
            .unwrap_or(&target);

        if options.inlines_basic {
            write!(w, "{text}")?;
        } else {
            write!(w, "<a href=\"{target}\">{text}</a>",)?;
        }
        Ok(())
    }
}

impl Render for Image {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        write!(
            w,
            "<img src=\"{}\"",
            match &self.source {
                ImageSource::Url(url) => url,
                ImageSource::Path(path) => path,
            }
        )?;
        if !self.title.is_empty() {
            write!(w, " alt=\"",)?;
            self.title
                .iter()
                .try_for_each(|node| node.render(w, processor, options))?;
            write!(w, "\"")?;
        }
        write!(w, "\">")?;
        Ok(())
    }
}

impl Render for Pass {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        _options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        if let Some(ref text) = self.text {
            let text = substitution_text(text);
            write!(w, "{text}")?;
        }
        Ok(())
    }
}

pub(crate) fn render_inlines(
    inlines: &[InlineNode],
    w: &mut impl Write,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), crate::Error> {
    for inline in inlines {
        inline.render(w, processor, options)?;
    }
    Ok(())
}

fn substitution_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('>', "&gt;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}
