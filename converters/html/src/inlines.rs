use std::io::Write;

use acdc_parser::{
    Autolink, Button, CrossReference, Footnote, Icon, InlineMacro, InlineNode, Keyboard, Link,
    Menu, Pass, PassthroughKind, Stem, StemNotation, Substitution, Url,
};

use crate::{Processor, Render, RenderOptions};

impl Render for InlineNode {
    type Error = crate::Error;

    #[allow(clippy::too_many_lines)]
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        match self {
            InlineNode::PlainText(p) => {
                let text = substitution_text(&p.content, options);
                write!(w, "{text}")?;
            }
            InlineNode::RawText(r) => {
                write!(w, "{}", r.content)?;
            }
            InlineNode::VerbatimText(v) => {
                // Process callout markers in verbatim text (used in literal and listing
                // blocks)
                let text = mark_callouts(&v.content);
                let text = replace_callout_placeholders(&text);
                write!(w, "{text}")?;
            }
            InlineNode::BoldText(b) => {
                if !options.inlines_basic {
                    match (&b.id, &b.role) {
                        (Some(id), Some(role)) => {
                            write!(w, "<strong id=\"{id}\" class=\"{role}\">")?;
                        }
                        (Some(id), None) => write!(w, "<strong id=\"{id}\">")?,
                        (None, Some(role)) => write!(w, "<strong class=\"{role}\">")?,
                        (None, None) => write!(w, "<strong>")?,
                    }
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
                    match (&i.id, &i.role) {
                        (Some(id), Some(role)) => write!(w, "<em id=\"{id}\" class=\"{role}\">")?,
                        (Some(id), None) => write!(w, "<em id=\"{id}\">")?,
                        (None, Some(role)) => write!(w, "<em class=\"{role}\">")?,
                        (None, None) => write!(w, "<em>")?,
                    }
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
                    match (&i.id, &i.role) {
                        (Some(id), Some(role)) => write!(w, "<mark id=\"{id}\" class=\"{role}\">")?,
                        (Some(id), None) => write!(w, "<mark id=\"{id}\">")?,
                        (None, Some(role)) => write!(w, "<mark class=\"{role}\">")?,
                        (None, None) => write!(w, "<mark>")?,
                    }
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
                    match (&m.id, &m.role) {
                        (Some(id), Some(role)) => write!(w, "<code id=\"{id}\" class=\"{role}\">")?,
                        (Some(id), None) => write!(w, "<code id=\"{id}\">")?,
                        (None, Some(role)) => write!(w, "<code class=\"{role}\">")?,
                        (None, None) => write!(w, "<code>")?,
                    }
                }
                for inline in &m.content {
                    inline.render(
                        w,
                        processor,
                        &RenderOptions {
                            inlines_basic: true,
                            ..*options
                        },
                    )?;
                }
                if !options.inlines_basic {
                    write!(w, "</code>")?;
                }
            }
            InlineNode::CurvedQuotationText(c) => {
                match (&c.id, &c.role) {
                    (Some(id), Some(role)) => {
                        write!(w, "<span id=\"{id}\" class=\"{role}\">&ldquo;")?;
                    }
                    (Some(id), None) => write!(w, "<span id=\"{id}\">&ldquo;")?,
                    (None, Some(role)) => write!(w, "<span class=\"{role}\">&ldquo;")?,
                    (None, None) => write!(w, "&ldquo;")?,
                }
                for inline in &c.content {
                    inline.render(w, processor, options)?;
                }
                if c.id.is_some() || c.role.is_some() {
                    write!(w, "&rdquo;</span>")?;
                } else {
                    write!(w, "&rdquo;")?;
                }
            }
            InlineNode::CurvedApostropheText(c) => {
                match (&c.id, &c.role) {
                    (Some(id), Some(role)) => {
                        write!(w, "<span id=\"{id}\" class=\"{role}\">&lsquo;")?;
                    }
                    (Some(id), None) => write!(w, "<span id=\"{id}\">&lsquo;")?,
                    (None, Some(role)) => write!(w, "<span class=\"{role}\">&lsquo;")?,
                    (None, None) => write!(w, "&lsquo;")?,
                }
                for inline in &c.content {
                    inline.render(w, processor, options)?;
                }
                if c.id.is_some() || c.role.is_some() {
                    write!(w, "&rsquo;</span>")?;
                } else {
                    write!(w, "&rsquo;")?;
                }
            }
            InlineNode::StandaloneCurvedApostrophe(_) => {
                write!(w, "&rsquo;")?;
            }
            InlineNode::SuperscriptText(s) => {
                match (&s.id, &s.role) {
                    (Some(id), Some(role)) => write!(w, "<sup id=\"{id}\" class=\"{role}\">")?,
                    (Some(id), None) => write!(w, "<sup id=\"{id}\">")?,
                    (None, Some(role)) => write!(w, "<sup class=\"{role}\">")?,
                    (None, None) => write!(w, "<sup>")?,
                }
                for inline in &s.content {
                    inline.render(w, processor, options)?;
                }
                write!(w, "</sup>")?;
            }
            InlineNode::SubscriptText(s) => {
                match (&s.id, &s.role) {
                    (Some(id), Some(role)) => write!(w, "<sub id=\"{id}\" class=\"{role}\">")?,
                    (Some(id), None) => write!(w, "<sub id=\"{id}\">")?,
                    (None, Some(role)) => write!(w, "<sub class=\"{role}\">")?,
                    (None, None) => write!(w, "<sub>")?,
                }
                for inline in &s.content {
                    inline.render(w, processor, options)?;
                }
                write!(w, "</sub>")?;
            }
            InlineNode::Macro(m) => m.render(w, processor, options)?,
            InlineNode::LineBreak(_) => {
                writeln!(w, "<br>")?;
            }
            InlineNode::InlineAnchor(anchor) => {
                write!(w, "<a id=\"{}\"></a>", anchor.id)?;
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    format!("Unsupported inline node: {self:?}"),
                )
                .into());
            }
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
            InlineMacro::Stem(s) => s.render(w, processor, options),
            InlineMacro::Icon(i) => i.render(w, processor, options),
            InlineMacro::Keyboard(k) => k.render(w, processor, options),
            InlineMacro::Menu(m) => m.render(w, processor, options),
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    format!("Unsupported inline macro: {self:?}"),
                )
                .into());
            }
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
            .map(|t| substitution_text(t, options))
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
        if processor.document_attributes.get("experimental").is_some() {
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
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        if let Some(ref text) = self.text {
            if self.substitutions.contains(&Substitution::SpecialChars)
                || self.kind == PassthroughKind::Single
                || self.kind == PassthroughKind::Double
            {
                let text = substitution_text(text, options);
                write!(w, "{text}")?;
            } else {
                write!(w, "{text}")?;
            }
        }
        Ok(())
    }
}

pub(crate) fn render_title(
    title: &[InlineNode],
    w: &mut impl Write,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), crate::Error> {
    // Only render title if not empty
    if !title.is_empty() {
        writeln!(w, "<div class=\"title\">")?;
        render_inlines(title, w, processor, options)?;
        writeln!(w, "</div>")?;
    }
    Ok(())
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

impl Render for Stem {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        _options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        match self.notation {
            StemNotation::Latexmath => {
                writeln!(w, "\\({}\\)", self.content)?;
            }
            StemNotation::Asciimath => {
                writeln!(w, "\\${}\\$", self.content)?;
            }
        }
        Ok(())
    }
}

impl Render for Icon {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        _options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        // Check icons attribute to determine rendering mode
        if let Some(icons_value) = processor.document_attributes.get("icons") {
            if icons_value.to_string() == "font" {
                // Font Awesome mode
                write!(w, "<span class=\"icon\"><i class=\"fa fa-{}\"></i></span>", self.target)?;
            } else {
                // Image mode (when icons attribute is set to something other than "font")
                write!(
                    w,
                    "<span class=\"image\"><img src=\"./images/icons/{}.png\" alt=\"{}\"></span>",
                    self.target, self.target
                )?;
            }
        } else {
            // Text mode (default when icons attribute is not set)
            write!(w, "<span class=\"icon\">[{}]</span>", self.target)?;
        }
        Ok(())
    }
}

impl Render for Keyboard {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        _options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        if self.keys.len() == 1 {
            // Single key
            write!(w, "<kbd>{}</kbd>", self.keys[0])?;
        } else {
            // Multiple keys
            write!(w, "<span class=\"keyseq\">")?;
            for (i, key) in self.keys.iter().enumerate() {
                if i > 0 {
                    write!(w, "+")?;
                }
                write!(w, "<kbd>{key}</kbd>")?;
            }
            write!(w, "</span>")?;
        }
        Ok(())
    }
}

impl Render for Menu {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        _options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        if self.items.is_empty() {
            // Simple menu reference
            write!(w, "<b class=\"menuref\">{}</b>", self.target)?;
        } else {
            // Menu selection with items
            write!(w, "<span class=\"menuseq\">")?;
            write!(w, "<b class=\"menu\">{}</b>", self.target)?;
            for (i, item) in self.items.iter().enumerate() {
                write!(w, "&#160;<i class=\"fa fa-angle-right caret\"></i> ")?;
                // Use "submenu" class for intermediate items, "menuitem" for the last
                if i == self.items.len() - 1 {
                    write!(w, "<b class=\"menuitem\">{item}</b>")?;
                } else {
                    write!(w, "<b class=\"submenu\">{item}</b>")?;
                }
            }
            write!(w, "</span>")?;
        }
        Ok(())
    }
}

fn substitution_text(text: &str, options: &RenderOptions) -> String {
    if text.is_empty() {
        return String::from("__EMPTY_WHEN_IT_SHOULD_NOT_BE__");
    }

    let text = text.replace("...", "&#8230;&#8203;");
    if options.inlines_basic {
        text
    } else {
        text.replace('&', "&amp;")
            .replace('>', "&gt;")
            .replace('<', "&lt;")
            .replace('"', "&quot;")
            .replace(" -- ", "&thinsp;&mdash;&thinsp;")
            .replace(" --", "&thinsp;&mdash;")
            .replace("-- ", "&mdash;&thinsp;")
    }
}

fn mark_callouts(text: &str) -> String {
    // Replace callout markers like <1>, <2> with placeholders
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            let mut num_str = String::new();

            while let Some(&next_char) = chars.peek() {
                if next_char.is_ascii_digit() {
                    num_str.push(next_char);
                    chars.next();
                } else if next_char == '>' && !num_str.is_empty() {
                    chars.next(); // consume the '>'
                    result.push_str("\u{FFFC}CALLOUT:");
                    result.push_str(&num_str);
                    result.push_str(":\u{FFFC}");
                    num_str.clear();
                    break;
                } else {
                    result.push('<');
                    result.push_str(&num_str);
                    break;
                }
            }

            if !num_str.is_empty() {
                result.push('<');
                result.push_str(&num_str);
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn replace_callout_placeholders(text: &str) -> String {
    // Replace callout placeholders with actual HTML
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\u{FFFC}' {
            // Check if this is a callout placeholder
            let mut placeholder = String::new();
            while let Some(&next_char) = chars.peek() {
                if next_char == '\u{FFFC}' {
                    chars.next();
                    break;
                }
                placeholder.push(next_char);
                chars.next();
            }

            if let Some(num_str) = placeholder
                .strip_prefix("CALLOUT:")
                .and_then(|s| s.strip_suffix(':'))
            {
                result.push_str("<b class=\"conum\">(");
                result.push_str(num_str);
                result.push_str(")</b>");
            } else {
                result.push('\u{FFFC}');
                result.push_str(&placeholder);
                result.push('\u{FFFC}');
            }
        } else {
            result.push(c);
        }
    }

    result
}
