use std::io::Write;

use acdc_parser::{
    Autolink, Button, CrossReference, Footnote, InlineMacro, InlineNode, Link, Pass,
    PassthroughKind, Substitution, Url,
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
                let text = substitution_text(&p.content);
                write!(w, "{text}")?;
            }
            InlineNode::RawText(r) => {
                write!(w, "{}", r.content)?;
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
            InlineNode::HighlightText(i) => {
                if !options.inlines_basic {
                    write!(w, "<mark>")?;
                }
                for inline in &i.content {
                    inline.render(w, processor, options)?;
                }
                if !options.inlines_basic {
                    write!(w, "</mark>")?;
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
            InlineNode::CurvedQuotationText(c) => {
                write!(w, "&ldquo;")?;
                for inline in &c.content {
                    inline.render(w, processor, options)?;
                }
                write!(w, "&rdquo;")?;
            }
            InlineNode::CurvedApostropheText(c) => {
                write!(w, "&lsquo;")?;
                for inline in &c.content {
                    inline.render(w, processor, options)?;
                }
                write!(w, "&rsquo;")?;
            }
            InlineNode::StandaloneCurvedApostrophe(_) => {
                write!(w, "&rsquo;")?;
            }
            InlineNode::Macro(m) => m.render(w, processor, options)?,
            unknown => todo!("inlines: {:?}", unknown),
        }
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
            InlineMacro::Autolink(al) => al.render(w, processor, options),
            InlineMacro::Link(l) => l.render(w, processor, options),
            InlineMacro::Image(i) => i.render(w, processor, options),
            InlineMacro::Pass(p) => p.render(w, processor, options),
            InlineMacro::Url(u) => u.render(w, processor, options),
            InlineMacro::Footnote(f) => f.render(w, processor, options),
            InlineMacro::Button(b) => b.render(w, processor, options),
            InlineMacro::CrossReference(xref) => xref.render(w, processor, options),
            unknown => todo!("inline macro: {:?}", unknown),
        }
    }
}

impl Render for Autolink {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        let content = &self.url;
        if options.inlines_basic {
            write!(w, "{content}")?;
        } else {
            write!(w, "<a href=\"{content}\">{content}</a>")?;
        }
        Ok(())
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
        let text = self
            .text
            .as_ref()
            .map(|t| substitution_text(t))
            .unwrap_or(format!("{}", self.target));
        if options.inlines_basic {
            write!(w, "{text}")?;
        } else {
            write!(w, "<a href=\"{}\">{text}</a>", self.target)?;
        }
        Ok(())
    }
}

impl Render for Button {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        _options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        // Buttons (UI macros) are experimental
        if processor
            .options
            .document_attributes
            .get("experimental")
            .is_some()
        {
            write!(w, "<b class=\"button\">{}</b>", self.label)?;
        } else {
            write!(w, "btn:[{}]", self.label)?;
        }
        Ok(())
    }
}

impl Render for CrossReference {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        if let Some(text) = &self.text {
            if options.inlines_basic {
                write!(w, "{text}")?;
            } else {
                write!(w, "<a href=\"#{}\">{text}</a>", self.target)?;
            }
        } else if options.inlines_basic {
            write!(w, "[{}]", self.target)?;
        } else {
            write!(w, "<a href=\"#{}\">[{}]</a>", self.target, self.target)?;
        }
        Ok(())
    }
}

impl Render for Url {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        write!(w, "<a href=\"{}\">", self.target)?;
        if self.text.is_empty() {
            write!(w, "{}", self.target)?;
        } else {
            crate::inlines::render_inlines(&self.text, w, processor, options)?;
        }
        write!(w, "</a>")?;
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
            if self.substitutions.contains(&Substitution::SpecialChars)
                || self.kind == PassthroughKind::Single
                || self.kind == PassthroughKind::Double
            {
                let text = substitution_text(text);
                write!(w, "{text}")?;
            } else {
                write!(w, "{text}")?;
            }
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

impl Render for Footnote {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        if options.inlines_basic {
            // In basic mode, just render the content
            write!(w, "[{}]", self.number)?;
        } else {
            let number = self.number;
            write!(w, "<sup class=\"footnote\"")?;
            if let Some(id) = &self.id {
                write!(w, " id=\"_footnote_{id}\"")?;
            }
            write!(
                w,
                ">[<a id=\"_footnoteref_{number}\" class=\"footnote\" href=\"#_footnotedef_{number}\" title=\"View footnote.\">{number}</a>]</sup>"
            )?;
            return Ok(());
        }
        Ok(())
    }
}

fn substitution_text(text: &str) -> String {
    if text.is_empty() {
        return String::from("__EMPTY_WHEN_IT_SHOULD_NOT_BE__");
    }
    text.replace('&', "&amp;")
        .replace('>', "&gt;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
        .replace(" -- ", "&thinsp;&mdash;&thinsp;")
        .replace(" --", "&thinsp;&mdash;")
        .replace("-- ", "&mdash;&thinsp;")
}
