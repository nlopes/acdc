//! Signature help: parameter hints for macro attribute lists

use tower_lsp_server::ls_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, Position,
    SignatureHelp, SignatureInformation,
};

use crate::state::DocumentState;

/// A single macro parameter (positional or named)
struct MacroParam {
    /// Display label (e.g., "alt", "width", "leveloffset=...")
    label: &'static str,
    /// Short documentation for the parameter
    doc: &'static str,
}

/// Signature definition for an `AsciiDoc` macro
struct MacroSignature {
    /// Macro name as it appears before the colon(s) (e.g., "image")
    name: &'static str,
    /// Human-readable description
    description: &'static str,
    /// Whether this macro has a block form (`::`)
    has_block_form: bool,
    /// Whether this macro has an inline form (`:`)
    has_inline_form: bool,
    /// Number of positional parameters (first N in `params`)
    positional_count: usize,
    /// Parameters in display order: positional first, then named
    params: &'static [MacroParam],
}

/// All supported macro signatures
const MACRO_SIGNATURES: &[MacroSignature] = &[
    MacroSignature {
        name: "image",
        description: "Image macro — embeds an image",
        has_block_form: true,
        has_inline_form: true,
        positional_count: 3,
        params: &[
            MacroParam {
                label: "alt",
                doc: "Alternative text for the image",
            },
            MacroParam {
                label: "width",
                doc: "Image width (e.g., 300, 50%)",
            },
            MacroParam {
                label: "height",
                doc: "Image height (e.g., 200, 50%)",
            },
            MacroParam {
                label: "id=...",
                doc: "Element ID",
            },
            MacroParam {
                label: "role=...",
                doc: "CSS role/class",
            },
            MacroParam {
                label: "title=...",
                doc: "Image title/caption",
            },
            MacroParam {
                label: "link=...",
                doc: "URL to link the image to",
            },
            MacroParam {
                label: "window=...",
                doc: "Link target window (e.g., _blank)",
            },
            MacroParam {
                label: "float=...",
                doc: "Float direction (left, right)",
            },
            MacroParam {
                label: "align=...",
                doc: "Alignment (left, center, right)",
            },
        ],
    },
    MacroSignature {
        name: "video",
        description: "Video macro — embeds a video",
        has_block_form: true,
        has_inline_form: false,
        positional_count: 3,
        params: &[
            MacroParam {
                label: "poster",
                doc: "Poster image URL (or 'youtube'/'vimeo' for hosted videos)",
            },
            MacroParam {
                label: "width",
                doc: "Video width",
            },
            MacroParam {
                label: "height",
                doc: "Video height",
            },
            MacroParam {
                label: "autoplay=...",
                doc: "Auto-play the video",
            },
            MacroParam {
                label: "loop=...",
                doc: "Loop the video",
            },
            MacroParam {
                label: "controls=...",
                doc: "Show player controls",
            },
            MacroParam {
                label: "muted=...",
                doc: "Mute the video",
            },
            MacroParam {
                label: "start=...",
                doc: "Start time in seconds",
            },
            MacroParam {
                label: "end=...",
                doc: "End time in seconds",
            },
            MacroParam {
                label: "theme=...",
                doc: "Player theme (YouTube only)",
            },
            MacroParam {
                label: "lang=...",
                doc: "Player language (YouTube only)",
            },
            MacroParam {
                label: "list=...",
                doc: "Playlist ID (YouTube only)",
            },
        ],
    },
    MacroSignature {
        name: "audio",
        description: "Audio macro — embeds audio",
        has_block_form: true,
        has_inline_form: false,
        positional_count: 0,
        params: &[
            MacroParam {
                label: "autoplay=...",
                doc: "Auto-play the audio",
            },
            MacroParam {
                label: "loop=...",
                doc: "Loop the audio",
            },
            MacroParam {
                label: "controls=...",
                doc: "Show player controls",
            },
            MacroParam {
                label: "muted=...",
                doc: "Mute the audio",
            },
        ],
    },
    MacroSignature {
        name: "icon",
        description: "Icon macro — displays an icon",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[
            MacroParam {
                label: "size",
                doc: "Icon size (1x, 2x, 3x, 4x, 5x, lg, fw)",
            },
            MacroParam {
                label: "title=...",
                doc: "Icon title/tooltip",
            },
            MacroParam {
                label: "role=...",
                doc: "CSS role/class",
            },
            MacroParam {
                label: "flip=...",
                doc: "Flip direction (horizontal, vertical)",
            },
            MacroParam {
                label: "rotate=...",
                doc: "Rotation angle (90, 180, 270)",
            },
            MacroParam {
                label: "link=...",
                doc: "URL to link the icon to",
            },
            MacroParam {
                label: "window=...",
                doc: "Link target window",
            },
        ],
    },
    MacroSignature {
        name: "link",
        description: "Link macro — creates a hyperlink",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[
            MacroParam {
                label: "text",
                doc: "Link display text",
            },
            MacroParam {
                label: "id=...",
                doc: "Element ID",
            },
            MacroParam {
                label: "role=...",
                doc: "CSS role/class",
            },
            MacroParam {
                label: "title=...",
                doc: "Link title/tooltip",
            },
            MacroParam {
                label: "window=...",
                doc: "Link target window (e.g., _blank)",
            },
            MacroParam {
                label: "opts=...",
                doc: "Additional options (e.g., nofollow)",
            },
        ],
    },
    MacroSignature {
        name: "mailto",
        description: "Mailto macro — creates an email link",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[
            MacroParam {
                label: "text",
                doc: "Link display text",
            },
            MacroParam {
                label: "subject=...",
                doc: "Email subject line",
            },
            MacroParam {
                label: "body=...",
                doc: "Email body text",
            },
        ],
    },
    MacroSignature {
        name: "include",
        description: "Include directive — includes content from another file",
        has_block_form: true,
        has_inline_form: false,
        positional_count: 0,
        params: &[
            MacroParam {
                label: "leveloffset=...",
                doc: "Adjust section level offset (e.g., +1, -1)",
            },
            MacroParam {
                label: "lines=...",
                doc: "Line ranges to include (e.g., 1..5, 1;3;5)",
            },
            MacroParam {
                label: "tag=...",
                doc: "Include tagged region",
            },
            MacroParam {
                label: "tags=...",
                doc: "Include multiple tagged regions",
            },
            MacroParam {
                label: "indent=...",
                doc: "Indent level for included content",
            },
            MacroParam {
                label: "encoding=...",
                doc: "Character encoding of included file",
            },
            MacroParam {
                label: "opts=...",
                doc: "Additional options (e.g., optional)",
            },
        ],
    },
    MacroSignature {
        name: "xref",
        description: "Cross-reference macro — links to another anchor",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[MacroParam {
            label: "text",
            doc: "Display text for the cross-reference",
        }],
    },
    MacroSignature {
        name: "kbd",
        description: "Keyboard macro — renders keyboard shortcuts",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[MacroParam {
            label: "keys",
            doc: "Key combination (e.g., Ctrl+C, Ctrl+Shift+T)",
        }],
    },
    MacroSignature {
        name: "btn",
        description: "Button macro — renders a UI button label",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[MacroParam {
            label: "label",
            doc: "Button label text",
        }],
    },
    MacroSignature {
        name: "menu",
        description: "Menu macro — renders a menu navigation path",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[MacroParam {
            label: "submenus",
            doc: "Submenu path (e.g., Save As... > PDF)",
        }],
    },
    MacroSignature {
        name: "footnote",
        description: "Footnote macro — adds a footnote",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[MacroParam {
            label: "content",
            doc: "Footnote text content",
        }],
    },
    MacroSignature {
        name: "pass",
        description: "Passthrough macro — passes content without processing",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[MacroParam {
            label: "content",
            doc: "Raw content to pass through",
        }],
    },
    MacroSignature {
        name: "stem",
        description: "STEM macro — renders a math formula",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[MacroParam {
            label: "formula",
            doc: "Mathematical formula",
        }],
    },
    MacroSignature {
        name: "latexmath",
        description: "LaTeX math macro — renders a LaTeX formula",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[MacroParam {
            label: "formula",
            doc: "LaTeX mathematical formula",
        }],
    },
    MacroSignature {
        name: "asciimath",
        description: "AsciiMath macro — renders an AsciiMath formula",
        has_block_form: false,
        has_inline_form: true,
        positional_count: 1,
        params: &[MacroParam {
            label: "formula",
            doc: "AsciiMath formula",
        }],
    },
];

/// Context detected from cursor position within a macro attribute list
struct MacroContext<'a> {
    /// The matched macro signature
    signature: &'a MacroSignature,
    /// Whether this is the block form (`::`) vs inline (`:`)
    is_block: bool,
    /// The active parameter index (0-based, from comma counting)
    active_param: u32,
}

/// Detect macro context from cursor position in raw text.
///
/// Scans backwards from the cursor to find an unmatched `[`, then looks for
/// a macro name pattern before it. Counts commas to determine active parameter.
fn detect_macro_context(text: &str, position: Position) -> Option<MacroContext<'_>> {
    let line_num = position.line as usize;
    let char_num = position.character as usize;

    let line = text.lines().nth(line_num)?;
    let before_cursor: String = line.chars().take(char_num).collect();
    let before_bytes = before_cursor.as_bytes();

    // Find the unmatched '[' by scanning backwards
    let bracket_pos = find_unmatched_open_bracket(before_bytes)?;

    // Extract text before the bracket to find the macro name
    let before_bracket = &before_cursor[..bracket_pos];

    // Look for macro pattern: word::target or word:target
    let (macro_name, is_block) = extract_macro_name(before_bracket)?;

    // Look up the macro signature
    let signature = MACRO_SIGNATURES.iter().find(|s| {
        s.name == macro_name && ((is_block && s.has_block_form) || (!is_block && s.has_inline_form))
    })?;

    // Count unquoted commas between '[' and cursor to determine active parameter
    let inside_brackets = &before_cursor[bracket_pos + 1..];
    let comma_count = count_unquoted_commas(inside_brackets);

    // For single-param macros (free text), clamp to 0
    let active_param = if signature.params.len() <= 1 {
        0
    } else {
        // Try to match named parameter if beyond positional count
        let idx = u32::try_from(comma_count).unwrap_or(u32::MAX);
        resolve_active_param(signature, inside_brackets, idx)
    };

    Some(MacroContext {
        signature,
        is_block,
        active_param,
    })
}

/// Find the byte position of the nearest unmatched `[` scanning backwards.
fn find_unmatched_open_bracket(bytes: &[u8]) -> Option<usize> {
    let mut depth: usize = 0;
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        match bytes.get(i) {
            Some(b']') => depth += 1,
            Some(b'[') => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Extract macro name and form (block vs inline) from text before `[`.
///
/// Matches patterns like `image::path/to/file.png` or `link:https://example.com`.
/// Validates the extracted name against known macro signatures to avoid false
/// positives from colons in targets (e.g., `https:` in URLs).
fn extract_macro_name(before_bracket: &str) -> Option<(&str, bool)> {
    // Try block form first: `word::...`
    if let Some(double_colon_pos) = before_bracket.rfind("::") {
        let before_colons = &before_bracket[..double_colon_pos];
        if let Some(name) = extract_trailing_word(before_colons)
            && is_known_macro(name)
        {
            return Some((name, true));
        }
    }

    // Try inline form: scan all single colons from right to left
    let bytes = before_bracket.as_bytes();
    let mut search_end = bytes.len();
    while let Some(rel_pos) = before_bracket.get(..search_end).and_then(|s| s.rfind(':')) {
        // Skip if this colon is part of `::` (preceded or followed by another colon)
        let preceded_by_colon = rel_pos > 0 && bytes.get(rel_pos - 1) == Some(&b':');
        let followed_by_colon = bytes.get(rel_pos + 1) == Some(&b':');
        if preceded_by_colon || followed_by_colon {
            search_end = rel_pos;
            continue;
        }

        if let Some(name) = before_bracket
            .get(..rel_pos)
            .and_then(extract_trailing_word)
            && is_known_macro(name)
        {
            return Some((name, false));
        }

        search_end = rel_pos;
    }

    None
}

/// Check if a name matches a known macro in `MACRO_SIGNATURES`.
fn is_known_macro(name: &str) -> bool {
    MACRO_SIGNATURES.iter().any(|s| s.name == name)
}

/// Extract the trailing alphabetic word from a string.
fn extract_trailing_word(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let end = bytes.len();
    if end == 0 {
        return None;
    }
    let mut start = end;
    while start > 0 && bytes.get(start - 1).is_some_and(u8::is_ascii_alphabetic) {
        start -= 1;
    }
    if start == end {
        None
    } else {
        Some(&s[start..end])
    }
}

/// Count commas that are not inside double quotes.
fn count_unquoted_commas(s: &str) -> usize {
    let mut count = 0;
    let mut in_quotes = false;
    for ch in s.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => count += 1,
            _ => {}
        }
    }
    count
}

/// Resolve the active parameter index, handling named parameter matching.
///
/// When the comma index exceeds the positional parameter count, tries to match
/// the current segment's `name=` prefix to a named parameter.
fn resolve_active_param(signature: &MacroSignature, inside_brackets: &str, comma_idx: u32) -> u32 {
    let param_count = u32::try_from(signature.params.len()).unwrap_or(u32::MAX);

    // If within positional range, use comma index directly
    if (comma_idx as usize) < signature.positional_count {
        return comma_idx.min(param_count.saturating_sub(1));
    }

    // Beyond positional params: try to match the current segment's name= prefix
    let current_segment = inside_brackets.rsplit(',').next().unwrap_or("");
    let trimmed = current_segment.trim_start();

    if let Some(eq_pos) = trimmed.find('=') {
        let typed_name = trimmed[..eq_pos].trim();
        // Search named parameters (those after positional_count)
        for (i, param) in signature
            .params
            .iter()
            .enumerate()
            .skip(signature.positional_count)
        {
            let param_name = param.label.trim_end_matches("=...");
            if param_name == typed_name {
                return u32::try_from(i).unwrap_or(comma_idx);
            }
        }
    }

    // Default: clamp to last param index
    comma_idx.min(param_count.saturating_sub(1))
}

/// Build the signature label string and collect parameter label offsets.
///
/// Returns `(label, offsets)` where offsets are `(start, end)` byte positions
/// for each parameter within the label.
fn build_signature_label(signature: &MacroSignature, is_block: bool) -> (String, Vec<(u32, u32)>) {
    let colon_sep = if is_block { "::" } else { ":" };
    let mut label = format!("{}{}target[", signature.name, colon_sep);
    let mut offsets = Vec::with_capacity(signature.params.len());

    for (i, param) in signature.params.iter().enumerate() {
        let start = u32::try_from(label.len()).unwrap_or(u32::MAX);
        label.push_str(param.label);
        let end = u32::try_from(label.len()).unwrap_or(u32::MAX);
        offsets.push((start, end));

        if i + 1 < signature.params.len() {
            label.push_str(", ");
        }
    }

    label.push(']');
    (label, offsets)
}

/// Compute signature help for a cursor position.
#[must_use]
pub fn compute_signature_help(doc: &DocumentState, position: Position) -> Option<SignatureHelp> {
    let ctx = detect_macro_context(&doc.text, position)?;

    let (label, offsets) = build_signature_label(ctx.signature, ctx.is_block);

    let parameters: Vec<ParameterInformation> = ctx
        .signature
        .params
        .iter()
        .zip(&offsets)
        .map(|(param, &(start, end))| ParameterInformation {
            label: ParameterLabel::LabelOffsets([start, end]),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::PlainText,
                value: param.doc.to_string(),
            })),
        })
        .collect();

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::PlainText,
                value: ctx.signature.description.to_string(),
            })),
            parameters: Some(parameters),
            active_parameter: Some(ctx.active_param),
        }],
        active_signature: Some(0),
        active_parameter: Some(ctx.active_param),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_text(text: &str) -> DocumentState {
        DocumentState::new_failure(text.to_string(), 0, vec![])
    }

    fn sig_help(text: &str, line: u32, character: u32) -> Option<SignatureHelp> {
        let doc = doc_with_text(text);
        compute_signature_help(&doc, Position { line, character })
    }

    fn assert_active_param(text: &str, line: u32, character: u32, expected: u32) {
        let help = sig_help(text, line, character);
        assert!(help.is_some(), "expected signature help for: {text}");
        if let Some(help) = help {
            assert_eq!(help.active_parameter, Some(expected));
        }
    }

    fn assert_label_starts_with(text: &str, line: u32, character: u32, prefix: &str) {
        let help = sig_help(text, line, character);
        assert!(help.is_some(), "expected signature help for: {text}");
        if let Some(help) = help {
            let label = help.signatures.first().map_or("", |s| s.label.as_str());
            assert!(
                label.starts_with(prefix),
                "expected label to start with '{prefix}', got '{label}'"
            );
        }
    }

    #[test]
    fn image_block_first_param() {
        assert_active_param("image::diagram.png[", 0, 19, 0);
        assert_label_starts_with("image::diagram.png[", 0, 19, "image::target[");
    }

    #[test]
    fn image_block_second_param() {
        assert_active_param("image::diagram.png[alt text,", 0, 28, 1);
    }

    #[test]
    fn image_block_third_param() {
        assert_active_param("image::diagram.png[alt text, 300,", 0, 33, 2);
    }

    #[test]
    fn image_inline_first_param() {
        assert_active_param("See image:photo.jpg[", 0, 20, 0);
        assert_label_starts_with("See image:photo.jpg[", 0, 20, "image:target[");
    }

    #[test]
    fn link_inline() {
        assert_active_param("link:https://example.com[", 0, 25, 0);
        assert_label_starts_with("link:https://example.com[", 0, 25, "link:target[");
    }

    #[test]
    fn include_named_params() {
        assert_active_param("include::chapter.adoc[leveloffset=+1,", 0, 37, 1);
    }

    #[test]
    fn include_match_named_param() {
        assert_active_param("include::chapter.adoc[leveloffset=+1, lines=", 0, 45, 1);
    }

    #[test]
    fn kbd_single_param_stays_at_zero() {
        assert_active_param("kbd:[Ctrl+", 0, 10, 0);
    }

    #[test]
    fn btn_single_param() {
        assert_active_param("btn:[OK", 0, 7, 0);
    }

    #[test]
    fn pass_single_param() {
        assert_active_param("pass:[<b>bold</b>", 0, 17, 0);
    }

    #[test]
    fn stem_single_param() {
        assert_active_param("stem:[x^2", 0, 9, 0);
    }

    #[test]
    fn no_match_outside_brackets() {
        assert!(sig_help("image::diagram.png", 0, 18).is_none());
    }

    #[test]
    fn no_match_after_closing_bracket() {
        assert!(sig_help("image::diagram.png[alt] more text", 0, 30).is_none());
    }

    #[test]
    fn no_match_unknown_macro() {
        assert!(sig_help("unknown::target[", 0, 16).is_none());
    }

    #[test]
    fn video_block_form() {
        assert_active_param("video::intro.mp4[", 0, 18, 0);
        assert_label_starts_with("video::intro.mp4[", 0, 18, "video::target[");
    }

    #[test]
    fn audio_block_form() {
        assert_active_param("audio::music.mp3[", 0, 17, 0);
    }

    #[test]
    fn icon_inline() {
        assert_active_param("icon:heart[", 0, 11, 0);
    }

    #[test]
    fn xref_inline() {
        assert_active_param("xref:section-1[", 0, 15, 0);
    }

    #[test]
    fn mailto_inline() {
        assert_active_param("mailto:user@example.com[", 0, 24, 0);
    }

    #[test]
    fn quoted_commas_not_counted() {
        assert_active_param(r#"image::photo.jpg["alt, with comma","#, 0, 35, 1);
    }

    #[test]
    fn multiline_not_supported() {
        let text = "image::diagram.png\n[alt text";
        assert!(sig_help(text, 1, 9).is_none());
    }

    #[test]
    fn video_no_inline_form() {
        assert!(sig_help("video:intro.mp4[", 0, 16).is_none());
    }

    #[test]
    fn toc_no_params() {
        assert!(sig_help("toc::[", 0, 6).is_none());
    }

    #[test]
    fn image_with_prefix_text() {
        assert_active_param("Check this image:photo.jpg[", 0, 27, 0);
    }

    #[test]
    fn find_unmatched_bracket_simple() {
        assert_eq!(find_unmatched_open_bracket(b"image::foo["), Some(10));
    }

    #[test]
    fn find_unmatched_bracket_nested() {
        assert_eq!(find_unmatched_open_bracket(b"image::foo[a[b]"), Some(10));
    }

    #[test]
    fn find_unmatched_bracket_none() {
        assert_eq!(find_unmatched_open_bracket(b"no brackets here"), None);
    }

    #[test]
    fn find_unmatched_bracket_all_matched() {
        assert_eq!(find_unmatched_open_bracket(b"[matched] text"), None);
    }

    #[test]
    fn extract_macro_name_block() {
        assert_eq!(
            extract_macro_name("image::diagram.png"),
            Some(("image", true))
        );
    }

    #[test]
    fn extract_macro_name_inline() {
        assert_eq!(
            extract_macro_name("link:https://example.com"),
            Some(("link", false))
        );
    }

    #[test]
    fn extract_macro_name_with_prefix() {
        assert_eq!(
            extract_macro_name("See image:photo.jpg"),
            Some(("image", false))
        );
    }

    #[test]
    fn extract_macro_name_none() {
        assert_eq!(extract_macro_name("no macro here"), None);
    }

    #[test]
    fn count_commas_simple() {
        assert_eq!(count_unquoted_commas("a, b, c"), 2);
    }

    #[test]
    fn count_commas_quoted() {
        assert_eq!(count_unquoted_commas(r#""a, b", c"#), 1);
    }

    #[test]
    fn count_commas_empty() {
        assert_eq!(count_unquoted_commas(""), 0);
    }

    #[test]
    fn build_label_image_block() -> Result<(), Box<dyn std::error::Error>> {
        let sig = MACRO_SIGNATURES
            .iter()
            .find(|s| s.name == "image")
            .ok_or("image macro not found")?;
        let (label, offsets) = build_signature_label(sig, true);
        assert!(label.starts_with("image::target["));
        assert!(label.ends_with(']'));
        assert_eq!(offsets.len(), sig.params.len());
        let &(start, end) = offsets.first().ok_or("expected at least one offset")?;
        assert_eq!(label.get(start as usize..end as usize), Some("alt"));
        Ok(())
    }

    #[test]
    fn build_label_link_inline() -> Result<(), Box<dyn std::error::Error>> {
        let sig = MACRO_SIGNATURES
            .iter()
            .find(|s| s.name == "link")
            .ok_or("link macro not found")?;
        let (label, offsets) = build_signature_label(sig, false);
        assert!(label.starts_with("link:target["));
        assert_eq!(offsets.len(), sig.params.len());
        Ok(())
    }
}
