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

use std::io::{self, Write};

use acdc_converters_common::{
    substitutions::{restore_escaped_patterns, strip_backslash_escapes},
    visitor::WritableVisitor,
};
use acdc_parser::{InlineMacro, InlineNode, StemNotation, Substitution, inlines_to_string};

use crate::{
    Error, Processor, RenderOptions,
    icon::write_icon,
    image_helpers::{alt_text_from_filename, write_dimension_attributes},
};

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
            // Warn about deprecated built-in roles
            if let Some(ref role) = h.role {
                for r in role.split_whitespace() {
                    match r {
                        "big" => tracing::warn!(
                            role = %r,
                            "Role is deprecated. Consider using `+++<big>+++text+++</big>+++` or CSS font-size instead."
                        ),
                        "small" => tracing::warn!(
                            role = %r,
                            "Role is deprecated. Consider using `+++<small>+++text+++</small>+++` or CSS font-size instead."
                        ),
                        _ => {}
                    }
                }
            }
            if !options.inlines_basic {
                // asciidoctor behavior: use <span> when role is present, <mark> otherwise
                let tag = if h.role.is_some() { "span" } else { "mark" };
                write_tag_with_attrs(w, tag, h.id.as_ref(), h.role.as_ref())?;
            }
            visitor.visit_inline_nodes(&h.content)?;
            if !options.inlines_basic {
                let w = visitor.writer_mut();
                let tag = if h.role.is_some() { "span" } else { "mark" };
                write!(w, "</{tag}>")?;
            }
        }
        InlineNode::MonospaceText(m) => {
            if !options.inlines_basic {
                write_tag_with_attrs(w, "code", m.id.as_ref(), m.role.as_ref())?;
            }
            visitor.visit_inline_nodes(&m.content)?;
            if !options.inlines_basic {
                let w = visitor.writer_mut();
                write!(w, "</code>")?;
            }
        }
        InlineNode::CurvedQuotationText(c) => {
            if c.id.is_some() || c.role.is_some() {
                write_tag_with_attrs(w, "span", c.id.as_ref(), c.role.as_ref())?;
                write!(w, "&ldquo;")?;
            } else {
                write!(w, "&ldquo;")?;
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
            if c.id.is_some() || c.role.is_some() {
                write_tag_with_attrs(w, "span", c.id.as_ref(), c.role.as_ref())?;
                write!(w, "&lsquo;")?;
            } else {
                write!(w, "&lsquo;")?;
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
            write_tag_with_attrs(w, "sup", s.id.as_ref(), s.role.as_ref())?;
            visitor.visit_inline_nodes(&s.content)?;
            let w = visitor.writer_mut();
            write!(w, "</sup>")?;
        }
        InlineNode::SubscriptText(s) => {
            write_tag_with_attrs(w, "sub", s.id.as_ref(), s.role.as_ref())?;
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
            let href = &al.url;
            // For mailto: URLs, display just the email address without the mailto: prefix
            let display_text = {
                let url_str = href.to_string();
                let url_str = if al.bracketed {
                    url_str
                        .strip_prefix('<')
                        .and_then(|s| s.strip_suffix('>'))
                        .unwrap_or(&url_str)
                } else {
                    &url_str
                };
                if let Some(email) = url_str.strip_prefix("mailto:") {
                    email.to_string()
                } else {
                    url_str.to_string()
                }
            };

            if options.inlines_basic {
                write!(w, "{display_text}")?;
            } else if al.bracketed {
                // Preserve angle brackets for bracketed autolinks (e.g., <user@example.com>)
                write!(w, "&lt;<a href=\"{href}\">{display_text}</a>&gt;")?;
            } else {
                write!(w, "<a href=\"{href}\">{display_text}</a>")?;
            }
        }
        InlineMacro::Link(l) => {
            let text = l
                .text
                .as_ref()
                .map(|t| substitution_text(t, options))
                .filter(|t| !t.is_empty()) // Treat empty string as None
                .unwrap_or_else(|| {
                    // For mailto: links without custom text, show just the email address
                    let target_str = l.target.to_string();
                    target_str
                        .strip_prefix("mailto:")
                        .unwrap_or(&target_str)
                        .to_string()
                });
            if options.inlines_basic {
                write!(w, "{text}")?;
            } else {
                write!(w, "<a href=\"{}\">{text}</a>", l.target)?;
            }
        }
        InlineMacro::Image(i) => {
            // Inline images use a span wrapper with the img tag inside
            write!(w, "<span class=\"image\">")?;

            // Get alt text from title field first (first positional attribute),
            // then fall back to alt attribute, then filename
            let alt_text = if i.title.is_empty() {
                i.metadata.attributes.get("alt").map_or_else(
                    || alt_text_from_filename(&i.source),
                    std::string::ToString::to_string,
                )
            } else {
                inlines_to_string(&i.title)
            };

            // Wrap in link if link attribute exists
            let link = i.metadata.attributes.get("link");
            if let Some(link) = link {
                write!(w, "<a class=\"image\" href=\"{link}\">")?;
            }

            // Write the img tag with src, alt, and dimensions
            write!(w, "<img src=\"{}\" alt=\"{alt_text}\"", i.source)?;
            write_dimension_attributes(w, &i.metadata)?;

            // Add title attribute for hover text if present (inline-specific)
            if let Some(title) = i.metadata.attributes.get("title") {
                write!(w, " title=\"{title}\"")?;
            }

            write!(w, ">")?;

            if link.is_some() {
                write!(w, "</a>")?;
            }
            write!(w, "</span>")?;
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
                // For mailto: URLs, display just the email address without the mailto: prefix
                let target_str = u.target.to_string();
                let display = target_str.strip_prefix("mailto:").unwrap_or(&target_str);
                write!(w, "{display}")?;
            } else {
                for inline in &u.text {
                    visit_inline_node(inline, visitor, processor, options)?;
                }
            }
            let w = visitor.writer_mut();
            write!(w, "</a>")?;
        }
        InlineMacro::Mailto(m) => {
            // Check for role attribute to apply as CSS class
            let class_attr = m
                .attributes
                .get("role")
                .and_then(|v| match v {
                    acdc_parser::AttributeValue::String(s) => {
                        let role = s.trim_matches('"');
                        if role.is_empty() {
                            None
                        } else {
                            Some(format!(" class=\"{role}\""))
                        }
                    }
                    acdc_parser::AttributeValue::Bool(_)
                    | acdc_parser::AttributeValue::None
                    | _ => None,
                })
                .unwrap_or_default();
            write!(w, "<a href=\"{}\"{class_attr}>", m.target)?;
            if m.text.is_empty() {
                // For mailto: URLs, display just the email address without the mailto: prefix
                let target_str = m.target.to_string();
                let display = target_str.strip_prefix("mailto:").unwrap_or(&target_str);
                write!(w, "{display}")?;
            } else {
                for inline in &m.text {
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
            } else {
                // Look up section from toc_entries
                // Priority: xreflabel (from [[id,Custom Text]]) > section title > fallback
                let display_text = processor
                    .toc_entries()
                    .iter()
                    .find(|entry| entry.id == xref.target)
                    .map_or_else(
                        || format!("[{}]", xref.target),
                        |entry| {
                            entry
                                .xreflabel
                                .clone()
                                .unwrap_or_else(|| inlines_to_string(&entry.title))
                        },
                    );

                if options.inlines_basic {
                    write!(w, "{display_text}")?;
                } else {
                    write!(w, "<a href=\"#{}\">{display_text}</a>", xref.target)?;
                }
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
            write_icon(w, processor, i)?;
        }
        InlineMacro::Keyboard(k) => {
            if k.keys.len() == 1
                && let Some(key) = k.keys.first()
            {
                write!(w, "<kbd>{key}</kbd>")?;
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

    // Strip backslash escapes first (before any other processing)
    let text = if options.inlines_basic || options.inlines_verbatim {
        text.to_string()
    } else {
        strip_backslash_escapes(text)
    };

    // Escape & first (before arrow replacements that produce & entities)
    let text = text.replace('&', "&amp;");

    // Apply arrow and dash substitutions before escaping < and >
    // (arrow patterns contain these characters)
    let text = if options.inlines_basic || options.inlines_verbatim {
        text
    } else {
        text.replace(" -- ", "&thinsp;&mdash;&thinsp;")
            .replace(" --", "&thinsp;&mdash;")
            .replace("-- ", "&mdash;&thinsp;")
            // Arrow replacements (double arrows first to avoid partial matches)
            .replace("=>", "&#8658;") // ⇒ rightwards double arrow
            .replace("<=", "&#8656;") // ⇐ leftwards double arrow
            .replace("->", "&#8594;") // → rightwards arrow
            .replace("<-", "&#8592;") // ← leftwards arrow
    };

    // Now escape remaining < and > (after arrow patterns have been replaced)
    // and apply typography transformations (ellipsis, smart quotes)
    let text = text
        .replace('>', "&gt;")
        .replace('<', "&lt;")
        .replace("...", "&#8230;&#8203;")
        .replace('\'', "&#8217;");

    // Restore escaped patterns (convert placeholders back to literal forms)
    // This must happen after typography substitutions to preserve escapes like \...
    if options.inlines_basic || options.inlines_verbatim {
        text
    } else {
        restore_escaped_patterns(&text)
    }
}

fn mark_callouts(text: &str) -> String {
    // Replace callout markers like <1>, <2>, or <.> with placeholders
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut auto_number = 1; // Counter for <.> auto-numbering

    while let Some(c) = chars.next() {
        if c == '<' {
            // Check for <.> pattern first
            if chars.peek() == Some(&'.') {
                chars.next(); // consume the '.'
                if chars.peek() == Some(&'>') {
                    chars.next(); // consume the '>'
                    result.push_str("\u{FFFC}CALLOUT:");
                    result.push_str(&auto_number.to_string());
                    result.push_str(":\u{FFFC}");
                    auto_number += 1;
                    continue;
                }
                // Not a valid <.> pattern, output what we consumed
                result.push('<');
                result.push('.');
                continue;
            }

            // Check for <digits> pattern
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
