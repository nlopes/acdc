//! Inline content rendering for HTML conversion.
//!
//! # Security Model and HTML Escaping
//!
//! `AsciiDoc` follows a **trusted document author** model, similar to Markdown. The document
//! author is trusted to create safe content, and certain features intentionally allow raw HTML
//! output for flexibility.
//!
//! ## Escaping Behavior by Content Type
//!
//! ### `PlainText`
//! - **Always escaped**: HTML special characters (`<`, `>`, `&`, `"`, `'`) are converted to entities
//! - **Typography applied**: Em-dashes, ellipses, and smart quotes (unless in basic/verbatim mode)
//! - Example: `<script>` → `&lt;script&gt;`
//!
//! ### `RawText` (Passthrough Content)
//! - **Never escaped by default**: Intentionally outputs raw HTML
//! - Used for passthrough blocks (`++++...++++`) and inline passthroughs (`pass:[]`, `+++...+++`)
//! - **Escaping when verbatim**: Only escaped when in verbatim context (listing/literal blocks)
//! - Example: `pass:[<strong>test</strong>]` → `<strong>test</strong>` (raw HTML)
//!
//! ### `VerbatimText` (Code Blocks)
//! - **Always escaped**: Used in listing and literal blocks to display code
//! - **Callout processing**: Handles `<1>`, `<2>` markers specially
//! - Example in listing block: `<h1>` → `&lt;h1&gt;`
//!
//! ### Passthrough with `SpecialChars` Substitution
//! - `pass:c[...]` or `pass:specialchars[...]` → HTML is escaped
//! - Example: `pass:c[<strong>test</strong>]` → `&lt;strong&gt;test&lt;/strong&gt;`
//!
//! ## Why This Design?
//!
//! This matches asciidoctor's security model where:
//! 1. Document authors are trusted (like Markdown, not like user-generated HTML)
//! 2. Passthrough is a feature for intentionally embedding raw HTML
//! 3. Regular content is always escaped for safety
//! 4. Code blocks escape HTML to display it correctly
//!
//! ## ID Attributes
//!
//! Currently, acdc auto-generates section IDs from titles rather than accepting custom IDs.
//! This is inherently safe. When custom ID support is added, asciidoctor does NOT escape
//! IDs, trusting the document author to provide safe values.

use std::{
    io::{self, Write},
    path::PathBuf,
};

use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{InlineMacro, InlineNode, StemNotation, Substitution};

use crate::{Error, Processor, RenderOptions};

/// Helper to write an HTML opening tag with optional id and role attributes.
///
/// Handles the common pattern of:
/// - Both id and role: `<tag id="..." class="...">`
/// - Only id: `<tag id="...">`
/// - Only role: `<tag class="...">`
/// - Neither: `<tag>`
fn write_tag_with_attrs<W: Write + ?Sized>(
    writer: &mut W,
    tag: &str,
    id: Option<&String>,
    role: Option<&String>,
) -> io::Result<()> {
    match (id, role) {
        (Some(id), Some(role)) => write!(writer, "<{tag} id=\"{id}\" class=\"{role}\">"),
        (Some(id), None) => write!(writer, "<{tag} id=\"{id}\">"),
        (None, Some(role)) => write!(writer, "<{tag} class=\"{role}\">"),
        (None, None) => write!(writer, "<{tag}>"),
    }
}

/// Internal implementation for visiting inline nodes
#[allow(clippy::too_many_lines)]
pub(crate) fn visit_inline_node<V: WritableVisitor<Error = Error> + ?Sized>(
    node: &InlineNode,
    visitor: &mut V,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), Error> {
    let w = visitor.writer_mut();
    match node {
        InlineNode::PlainText(p) => {
            // PlainText always gets escaping and typography substitutions
            let text = substitution_text(&p.content, options);
            write!(w, "{text}")?;
        }
        InlineNode::RawText(r) => {
            // RawText outputs as-is (no escaping, no substitutions) unless in verbatim mode
            let text = if options.inlines_verbatim {
                substitution_text(&r.content, options)
            } else {
                r.content.clone()
            };
            write!(w, "{text}")?;
        }
        InlineNode::VerbatimText(v) => {
            // VerbatimText handles callouts and escaping (verbatim mode always applies)
            let text = mark_callouts(&v.content);
            // Create temporary options with verbatim mode enabled for escaping
            let verbatim_options = RenderOptions {
                inlines_verbatim: true,
                ..options.clone()
            };
            // Apply HTML escaping and typography BEFORE replacing callout placeholders
            // This ensures the HTML tags in callouts don't get escaped
            let text = substitution_text(&text, &verbatim_options);
            let text = replace_callout_placeholders(&text);
            write!(w, "{text}")?;
        }
        InlineNode::BoldText(b) => {
            if !options.inlines_basic {
                write_tag_with_attrs(w, "strong", b.id.as_ref(), b.role.as_ref())?;
            }
            visitor.visit_inline_nodes(&b.content)?;
            let w = visitor.writer_mut();
            if !options.inlines_basic {
                write!(w, "</strong>")?;
            }
        }
        InlineNode::ItalicText(i) => {
            if !options.inlines_basic {
                write_tag_with_attrs(w, "em", i.id.as_ref(), i.role.as_ref())?;
            }
            visitor.visit_inline_nodes(&i.content)?;
            if !options.inlines_basic {
                let w = visitor.writer_mut();
                write!(w, "</em>")?;
            }
        }
        InlineNode::HighlightText(h) => {
            if !options.inlines_basic {
                write_tag_with_attrs(w, "mark", h.id.as_ref(), h.role.as_ref())?;
            }
            visitor.visit_inline_nodes(&h.content)?;
            if !options.inlines_basic {
                let w = visitor.writer_mut();
                write!(w, "</mark>")?;
            }
        }
        InlineNode::MonospaceText(m) => {
            if !options.inlines_basic {
                write_tag_with_attrs(w, "code", m.id.as_ref(), m.role.as_ref())?;
            }
            for inline in &m.content {
                visit_inline_node(
                    inline,
                    visitor,
                    processor,
                    &RenderOptions {
                        inlines_basic: true,
                        ..*options
                    },
                )?;
            }
            if !options.inlines_basic {
                let w = visitor.writer_mut();
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
            visitor.visit_inline_nodes(&c.content)?;
            let w = visitor.writer_mut();
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
            visitor.visit_inline_nodes(&c.content)?;
            let w = visitor.writer_mut();
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
            visitor.visit_inline_nodes(&s.content)?;
            let w = visitor.writer_mut();
            write!(w, "</sup>")?;
        }
        InlineNode::SubscriptText(s) => {
            match (&s.id, &s.role) {
                (Some(id), Some(role)) => write!(w, "<sub id=\"{id}\" class=\"{role}\">")?,
                (Some(id), None) => write!(w, "<sub id=\"{id}\">")?,
                (None, Some(role)) => write!(w, "<sub class=\"{role}\">")?,
                (None, None) => write!(w, "<sub>")?,
            }
            visitor.visit_inline_nodes(&s.content)?;
            let w = visitor.writer_mut();
            write!(w, "</sub>")?;
        }
        InlineNode::Macro(m) => render_inline_macro(m, visitor, processor, options)?,
        InlineNode::LineBreak(_) => {
            writeln!(w, "<br>")?;
        }
        InlineNode::InlineAnchor(anchor) => {
            write!(w, "<a id=\"{}\"></a>", anchor.id)?;
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("Unsupported inline node: {node:?}"),
            )
            .into());
        }
    }
    Ok(())
}

/// Render an inline macro
#[allow(clippy::too_many_lines)]
fn render_inline_macro<V: WritableVisitor<Error = Error> + ?Sized>(
    m: &InlineMacro,
    visitor: &mut V,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), Error> {
    let w = visitor.writer_mut();
    match m {
        InlineMacro::Autolink(al) => {
            let content = &al.url;
            if options.inlines_basic {
                write!(w, "{content}")?;
            } else {
                write!(w, "<a href=\"{content}\">{content}</a>")?;
            }
        }
        InlineMacro::Link(l) => {
            let text = l
                .text
                .as_ref()
                .map(|t| substitution_text(t, options))
                .unwrap_or(format!("{}", l.target));
            if options.inlines_basic {
                write!(w, "{text}")?;
            } else {
                write!(w, "<a href=\"{}\">{text}</a>", l.target)?;
            }
        }
        InlineMacro::Image(i) => {
            // Inline images are simpler than block images - just the img tag
            let link = i.metadata.attributes.get("link");
            if let Some(link) = link {
                write!(w, "<a class=\"image\" href=\"{link}\">")?;
            }
            write!(w, "<img src=\"{}\"", i.source)?;
            if let Some(alt) = i.metadata.attributes.get("alt") {
                write!(w, " alt=\"{alt}\"")?;
            } else {
                // If no alt text is provided, take the filename without the extension
                let mut filepath = PathBuf::from(i.source.get_filename().unwrap_or(""));
                filepath.set_extension("");
                write!(
                    w,
                    " alt=\"{}\"",
                    filepath.to_str().unwrap_or("").replace(['-', '_'], " ")
                )?;
            }
            write!(w, " />")?;
            if link.is_some() {
                write!(w, "</a>")?;
            }
        }
        InlineMacro::Pass(p) => {
            if let Some(ref text) = p.text {
                // Only escape when SpecialChars substitution is enabled (pass:c[])
                // Otherwise output raw HTML (pass:[], +++...+++)
                if p.substitutions.contains(&Substitution::SpecialChars) {
                    let text = substitution_text(text, options);
                    write!(w, "{text}")?;
                } else {
                    write!(w, "{text}")?;
                }
            }
        }
        InlineMacro::Url(u) => {
            write!(w, "<a href=\"{}\">", u.target)?;
            if u.text.is_empty() {
                write!(w, "{}", u.target)?;
            } else {
                for inline in &u.text {
                    visit_inline_node(inline, visitor, processor, options)?;
                }
            }
            let w = visitor.writer_mut();
            write!(w, "</a>")?;
        }
        InlineMacro::Footnote(f) => {
            if options.inlines_basic {
                write!(w, "[{}]", f.number)?;
            } else {
                let number = f.number;
                write!(w, "<sup class=\"footnote\"")?;
                if let Some(id) = &f.id {
                    write!(w, " id=\"_footnote_{id}\"")?;
                }
                write!(
                    w,
                    ">[<a id=\"_footnoteref_{number}\" class=\"footnote\" href=\"#_footnotedef_{number}\" title=\"View footnote.\">{number}</a>]</sup>"
                )?;
            }
        }
        InlineMacro::Button(b) => {
            if processor.document_attributes.get("experimental").is_some() {
                write!(w, "<b class=\"button\">{}</b>", b.label)?;
            } else {
                write!(w, "btn:[{}]", b.label)?;
            }
        }
        InlineMacro::CrossReference(xref) => {
            if let Some(text) = &xref.text {
                if options.inlines_basic {
                    write!(w, "{text}")?;
                } else {
                    write!(w, "<a href=\"#{}\">{text}</a>", xref.target)?;
                }
            } else if options.inlines_basic {
                write!(w, "[{}]", xref.target)?;
            } else {
                write!(w, "<a href=\"#{}\">[{}]</a>", xref.target, xref.target)?;
            }
        }
        InlineMacro::Stem(s) => match s.notation {
            StemNotation::Latexmath => {
                writeln!(w, "\\({}\\)", s.content)?;
            }
            StemNotation::Asciimath => {
                writeln!(w, "\\${}\\$", s.content)?;
            }
        },
        InlineMacro::Icon(i) => {
            if let Some(icons_value) = processor.document_attributes.get("icons") {
                if icons_value.to_string() == "font" {
                    write!(
                        w,
                        "<span class=\"icon\"><i class=\"fa fa-{}\"></i></span>",
                        i.target
                    )?;
                } else {
                    write!(
                        w,
                        "<span class=\"image\"><img src=\"./images/icons/{}.png\" alt=\"{}\"></span>",
                        i.target, i.target
                    )?;
                }
            } else {
                write!(w, "<span class=\"icon\">[{}]</span>", i.target)?;
            }
        }
        InlineMacro::Keyboard(k) => {
            if k.keys.len() == 1 {
                write!(w, "<kbd>{}</kbd>", k.keys[0])?;
            } else {
                write!(w, "<span class=\"keyseq\">")?;
                for (i, key) in k.keys.iter().enumerate() {
                    if i > 0 {
                        write!(w, "+")?;
                    }
                    write!(w, "<kbd>{key}</kbd>")?;
                }
                write!(w, "</span>")?;
            }
        }
        InlineMacro::Menu(menu) => {
            if menu.items.is_empty() {
                write!(w, "<b class=\"menuref\">{}</b>", menu.target)?;
            } else {
                write!(w, "<span class=\"menuseq\">")?;
                write!(w, "<b class=\"menu\">{}</b>", menu.target)?;
                for (i, item) in menu.items.iter().enumerate() {
                    write!(w, "&#160;<i class=\"fa fa-angle-right caret\"></i> ")?;
                    if i == menu.items.len() - 1 {
                        write!(w, "<b class=\"menuitem\">{item}</b>")?;
                    } else {
                        write!(w, "<b class=\"submenu\">{item}</b>")?;
                    }
                }
                write!(w, "</span>")?;
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("Unsupported inline macro: {m:?}"),
            )
            .into());
        }
    }
    Ok(())
}

fn substitution_text(text: &str, options: &RenderOptions) -> String {
    if text.is_empty() {
        return String::from("__EMPTY_WHEN_IT_SHOULD_NOT_BE__");
    }

    // Escape HTML special characters that are dangerous (<, >, &)
    // and apply typography transformations (ellipsis, smart quotes)
    let text = text
        .replace('&', "&amp;")
        .replace('>', "&gt;")
        .replace('<', "&lt;")
        .replace("...", "&#8230;&#8203;")
        .replace('\'', "&#8217;"); // Convert straight apostrophe to curly (smart quote)

    // Apply additional text transformations only when not in basic or verbatim mode
    if options.inlines_basic || options.inlines_verbatim {
        text
    } else {
        text.replace(" -- ", "&thinsp;&mdash;&thinsp;")
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
