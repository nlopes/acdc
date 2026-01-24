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

use acdc_converters_core::{
    substitutions::{restore_escaped_patterns, strip_backslash_escapes},
    visitor::WritableVisitor,
};
use acdc_parser::{
    Form, InlineMacro, InlineNode, StemNotation, Substitution, inlines_to_string,
    parse_text_for_quotes, substitute,
};

use crate::{
    Error, Processor, RenderOptions,
    constants::encode_html_entities,
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

/// Tracks what was written by `write_quote_open` so `write_quote_close` can match.
#[derive(Clone, Copy)]
enum QuoteRenderState {
    /// Wrote HTML tag - close with `</tag>`
    Html,
    /// Basic mode - no tags written, no closing needed
    Basic,
    /// Quotes disabled - wrote literal delimiter, close with same
    Literal,
}

/// Write opening markup for inline formatting (bold, italic, etc.).
///
/// Returns state indicating what was written, for use with `write_quote_close`.
fn write_quote_open<W: Write + ?Sized>(
    w: &mut W,
    tag: &str,
    delim: &str,
    id: Option<&String>,
    role: Option<&String>,
    subs: &[Substitution],
    basic: bool,
) -> io::Result<QuoteRenderState> {
    if subs.contains(&Substitution::Quotes) {
        if basic {
            Ok(QuoteRenderState::Basic)
        } else {
            write_tag_with_attrs(w, tag, id, role)?;
            Ok(QuoteRenderState::Html)
        }
    } else {
        write!(w, "{delim}")?;
        Ok(QuoteRenderState::Literal)
    }
}

/// Write closing markup for inline formatting.
fn write_quote_close<W: Write + ?Sized>(
    w: &mut W,
    tag: &str,
    delim: &str,
    state: QuoteRenderState,
) -> io::Result<()> {
    match state {
        QuoteRenderState::Html => write!(w, "</{tag}>"),
        QuoteRenderState::Basic => Ok(()),
        QuoteRenderState::Literal => write!(w, "{delim}"),
    }
}

/// Internal implementation for visiting inline nodes
#[allow(clippy::too_many_lines)]
pub(crate) fn visit_inline_node<V: WritableVisitor<Error = Error> + ?Sized>(
    node: &InlineNode,
    visitor: &mut V,
    processor: &Processor,
    options: &RenderOptions,
    subs: &[Substitution],
) -> Result<(), Error> {
    let w = visitor.writer_mut();
    // Helper to apply attribute substitution if enabled
    let maybe_substitute_attrs = |content: &str| -> String {
        if subs.contains(&Substitution::Attributes) {
            substitute(
                content,
                &[Substitution::Attributes],
                processor.document_attributes(),
            )
        } else {
            content.to_string()
        }
    };
    match node {
        InlineNode::PlainText(p) => {
            // Attribute substitution already applied by inline preprocessor during parsing
            let content = &p.content;

            // If escaped (from `\^2^` etc.), skip quote re-parsing; otherwise use block subs.
            let effective_subs: &[Substitution] = if p.escaped { &[] } else { subs };

            if effective_subs.contains(&Substitution::Quotes) {
                // If quotes substitution is enabled, parse for inline formatting
                let parsed_nodes = parse_text_for_quotes(content);
                // Render parsed nodes without quotes to avoid infinite recursion
                let no_quotes_subs: Vec<_> = effective_subs
                    .iter()
                    .filter(|s| **s != Substitution::Quotes)
                    .cloned()
                    .collect();
                for node in &parsed_nodes {
                    visit_inline_node(node, visitor, processor, options, &no_quotes_subs)?;
                }
            } else {
                // No quotes substitution - output with escaping and typography only
                let text = substitution_text(content, effective_subs, options);
                write!(w, "{text}")?;
            }
        }
        InlineNode::RawText(r) => {
            // RawText comes from passthroughs - attribute expansion was already
            // handled (or explicitly skipped) by the preprocessor based on the
            // passthrough's own substitution settings. Do NOT apply block subs.
            let content = &r.content;
            let text = if options.inlines_verbatim {
                substitution_text(content, subs, options)
            } else {
                content.clone()
            };
            write!(w, "{text}")?;
        }
        InlineNode::VerbatimText(v) => {
            // VerbatimText is now just text (callouts are separate CalloutRef nodes)
            // Apply attribute substitution first, then escaping
            let content = maybe_substitute_attrs(&v.content);
            let verbatim_options = RenderOptions {
                inlines_verbatim: true,
                ..options.clone()
            };

            // If quotes substitution is enabled, parse for inline formatting
            if subs.contains(&Substitution::Quotes) {
                let parsed_nodes = parse_text_for_quotes(&content);
                // Render parsed nodes with verbatim settings
                // Keep Quotes in subs so BoldText/ItalicText render as HTML
                for node in &parsed_nodes {
                    visit_inline_node(node, visitor, processor, &verbatim_options, subs)?;
                }
            } else {
                let text = substitution_text(&content, subs, &verbatim_options);
                write!(w, "{text}")?;
            }
        }
        InlineNode::CalloutRef(callout) => {
            // Render callout reference matching asciidoctor's format
            write!(
                w,
                "<i class=\"conum\" data-value=\"{0}\"></i><b>({0})</b>",
                callout.number
            )?;
        }
        InlineNode::BoldText(b) => {
            let delim = match b.form {
                Form::Constrained => "*",
                Form::Unconstrained => "**",
            };
            let state = write_quote_open(
                w,
                "strong",
                delim,
                b.id.as_ref(),
                b.role.as_ref(),
                subs,
                options.inlines_basic,
            )?;
            visitor.visit_inline_nodes(&b.content)?;
            write_quote_close(visitor.writer_mut(), "strong", delim, state)?;
        }
        InlineNode::ItalicText(i) => {
            let delim = match i.form {
                Form::Constrained => "_",
                Form::Unconstrained => "__",
            };
            let state = write_quote_open(
                w,
                "em",
                delim,
                i.id.as_ref(),
                i.role.as_ref(),
                subs,
                options.inlines_basic,
            )?;
            visitor.visit_inline_nodes(&i.content)?;
            write_quote_close(visitor.writer_mut(), "em", delim, state)?;
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
            // Check if quotes substitution is enabled
            if subs.contains(&Substitution::Quotes) {
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
            } else {
                // No quotes substitution - output raw markup
                let delim = match h.form {
                    Form::Constrained => "#",
                    Form::Unconstrained => "##",
                };
                write!(w, "{delim}")?;
                visitor.visit_inline_nodes(&h.content)?;
                let w = visitor.writer_mut();
                write!(w, "{delim}")?;
            }
        }
        InlineNode::MonospaceText(m) => {
            let delim = match m.form {
                Form::Constrained => "`",
                Form::Unconstrained => "``",
            };
            let state = write_quote_open(
                w,
                "code",
                delim,
                m.id.as_ref(),
                m.role.as_ref(),
                subs,
                options.inlines_basic,
            )?;
            visitor.visit_inline_nodes(&m.content)?;
            write_quote_close(visitor.writer_mut(), "code", delim, state)?;
        }
        InlineNode::CurvedQuotationText(c) => {
            // Check if quotes substitution is enabled
            if subs.contains(&Substitution::Quotes) {
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
            } else {
                // No quotes substitution - output literal quotes
                write!(w, "\"")?;
                visitor.visit_inline_nodes(&c.content)?;
                let w = visitor.writer_mut();
                write!(w, "\"")?;
            }
        }
        InlineNode::CurvedApostropheText(c) => {
            // Check if quotes substitution is enabled
            if subs.contains(&Substitution::Quotes) {
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
            } else {
                // No quotes substitution - output literal apostrophes
                write!(w, "'")?;
                visitor.visit_inline_nodes(&c.content)?;
                let w = visitor.writer_mut();
                write!(w, "'")?;
            }
        }
        InlineNode::StandaloneCurvedApostrophe(_) => {
            // Check if quotes substitution is enabled
            if subs.contains(&Substitution::Quotes) {
                write!(w, "&rsquo;")?;
            } else {
                write!(w, "'")?;
            }
        }
        InlineNode::SuperscriptText(s) => {
            // Note: superscript doesn't check inlines_basic (pass false to preserve behavior)
            let state =
                write_quote_open(w, "sup", "^", s.id.as_ref(), s.role.as_ref(), subs, false)?;
            visitor.visit_inline_nodes(&s.content)?;
            write_quote_close(visitor.writer_mut(), "sup", "^", state)?;
        }
        InlineNode::SubscriptText(s) => {
            // Note: subscript doesn't check inlines_basic (pass false to preserve behavior)
            let state =
                write_quote_open(w, "sub", "~", s.id.as_ref(), s.role.as_ref(), subs, false)?;
            visitor.visit_inline_nodes(&s.content)?;
            write_quote_close(visitor.writer_mut(), "sub", "~", state)?;
        }
        InlineNode::Macro(m) => render_inline_macro(m, visitor, processor, options, subs)?,
        InlineNode::LineBreak(_) => {
            writeln!(w, "<br>")?;
        }
        InlineNode::InlineAnchor(anchor) => {
            write!(w, "<a id=\"{}\"></a>", anchor.id)?;
        }
        // `InlineNode` is `#[non_exhaustive]`, so we need a catch-all for future variants
        _ => {}
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
    subs: &[Substitution],
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
                .map(|t| substitution_text(t, subs, options))
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
                    let text = substitution_text(text, subs, options);
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
                    visit_inline_node(inline, visitor, processor, options, subs)?;
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
                    visit_inline_node(inline, visitor, processor, options, subs)?;
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
            if xref.text.is_empty() {
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
            } else if options.inlines_basic {
                for inline in &xref.text {
                    visit_inline_node(inline, visitor, processor, options, subs)?;
                }
            } else {
                write!(w, "<a href=\"#{}\">", xref.target)?;
                for inline in &xref.text {
                    visit_inline_node(inline, visitor, processor, options, subs)?;
                }
                let w = visitor.writer_mut();
                write!(w, "</a>")?;
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
        InlineMacro::IndexTerm(it) => {
            // Generate anchor and collect entry for index catalog
            let anchor_id = processor.add_index_entry(it.kind.clone());

            // Output anchor for linking from index catalog
            write!(w, "<a id=\"{anchor_id}\"></a>")?;

            // Flow terms (visible): also output the term text
            if it.is_visible() {
                let text = substitution_text(it.term(), subs, options);
                write!(w, "{text}")?;
            }
            // Concealed terms: anchor only, no visible text
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

fn substitution_text(text: &str, subs: &[Substitution], options: &RenderOptions) -> String {
    debug_assert!(
        !text.is_empty(),
        "substitution_text called with empty text - caller should filter empty content"
    );
    if text.is_empty() {
        tracing::warn!(
            "substitution_text called with empty text - caller should filter empty content"
        );
        return String::new();
    }

    // When escape_html is false (subs=none), return text as-is
    if !subs.contains(&Substitution::SpecialChars) {
        return text.to_string();
    }

    // Determine if we should apply typography replacements
    // Based on substitutions list, skip for basic mode (passthrough)
    let should_apply_replacements =
        subs.contains(&Substitution::Replacements) && !options.inlines_basic;

    // Strip backslash escapes first (before any other processing)
    // Only needed when replacements are applied (escape sequences only matter for replacements)
    let text = if should_apply_replacements {
        strip_backslash_escapes(text)
    } else {
        text.to_string()
    };

    // Escape & first (before arrow replacements that produce & entities)
    let text = text.replace('&', "&amp;");

    // Apply arrow and dash substitutions before escaping < and >
    // (arrow patterns contain these characters)
    let text = if should_apply_replacements {
        text.replace(" -- ", "&thinsp;&mdash;&thinsp;")
            .replace(" --", "&thinsp;&mdash;")
            .replace("-- ", "&mdash;&thinsp;")
            // Arrow replacements (double arrows first to avoid partial matches)
            .replace("=>", "&#8658;") // ⇒ rightwards double arrow
            .replace("<=", "&#8656;") // ⇐ leftwards double arrow
            .replace("->", "&#8594;") // → rightwards arrow
            .replace("<-", "&#8592;") // ← leftwards arrow
    } else {
        text
    };

    // Now escape remaining < and > (after arrow patterns have been replaced)
    let text = text.replace('>', "&gt;").replace('<', "&lt;");

    // Apply typography transformations (ellipsis, smart quotes) only when replacements enabled
    let text = if should_apply_replacements {
        text.replace("...", "&#8230;&#8203;")
            .replace('\'', "&#8217;")
    } else {
        text
    };

    // Restore escaped patterns (convert placeholders back to literal forms)
    // This must happen after typography substitutions to preserve escapes like \...
    let text = if should_apply_replacements {
        restore_escaped_patterns(&text)
    } else {
        text
    };

    // Encode non-ASCII Unicode characters as HTML numeric entities
    // to match asciidoctor's output format
    encode_html_entities(&text)
}
