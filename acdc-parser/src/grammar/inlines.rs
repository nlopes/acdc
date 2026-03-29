// The `peg` macro adds 5 hidden parameters to every rule function, so even
// rules with just 3 explicit params exceed clippy's 7-argument threshold.
#![allow(clippy::too_many_arguments)]

use crate::{
    Anchor, AttributeValue, Autolink, BlockMetadata, Bold, Button, CurvedApostrophe,
    CurvedQuotation, Footnote, Form, Highlight, ICON_SIZES, Icon, Image, IndexTerm, IndexTermKind,
    InlineMacro, InlineNode, Italic, Keyboard, LineBreak, Link, Mailto, Menu, Monospace, Pass,
    PassthroughKind, Plain, Source, StandaloneCurvedApostrophe, Stem, StemNotation, Subscript,
    Superscript, Url,
    grammar::{
        ParserState, inline_preprocessing,
        inline_preprocessor::InlinePreprocessorParserState,
        inline_processing::{
            process_inlines, process_inlines_no_autolinks, process_inlines_preprocessed,
        },
    },
    model::{strip_quotes, substitution::HEADER},
};

use super::helpers::{
    AttributeProcessingMode, BlockParsingMetadata, PositionWithOffset, RESERVED_NAMED_ATTRIBUTE_ID,
    RESERVED_NAMED_ATTRIBUTE_OPTIONS, RESERVED_NAMED_ATTRIBUTE_ROLE, Shorthand,
    process_attribute_list, strip_url_backslash_escapes,
};

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub(crate) fn match_constrained_boundary(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'('
            | b'{'
            | b'['
            | b')'
            | b'}'
            | b']'
            | b'/'
            | b'-'
            | b'|'
            | b','
            | b';'
            | b'.'
            | b'?'
            | b'!'
            | b'\''
            | b'"'
            | b'<'
            | b'>'
            | b'^'
            | b'~'
    )
}

/**
Check whether the character before `pos` is a valid constrained opening boundary.

At position 0, falls back to `outer_delimiter` (the byte preceding the current
inline span in the parent context). A word-character outer delimiter means the
boundary is invalid.
*/
fn check_constrained_opening_boundary(
    pos: usize,
    input: &[u8],
    outer_delimiter: Option<u8>,
) -> bool {
    if pos == 0 {
        outer_delimiter.is_none_or(|d| !is_word_char(d))
    } else {
        input
            .get(pos - 1)
            .is_none_or(|&b| match_constrained_boundary(b))
    }
}

/**
Check whether a constrained closing delimiter at `end` is valid.

If `end` is at the end of the input, the outer delimiter must not be a word
character (otherwise the markup would be adjacent to a word character in the
parent context).
*/
fn check_constrained_closing_at_end(
    end: usize,
    input_len: usize,
    outer_delimiter: Option<u8>,
) -> bool {
    end < input_len || outer_delimiter.is_none_or(|d| !is_word_char(d))
}

/// Macro to handle inline processing errors with logging
macro_rules! process_inlines_or_err {
    ($call:expr, $msg:literal) => {
        $call.map_err(|e| {
            tracing::error!(?e, $msg);
            $msg
        })
    };
}

peg::parser! {
    pub(crate) grammar inline_parser(state: &mut ParserState) for str {
        use std::str::FromStr;
        use crate::model::{substitute, Substitution};
        use crate::model::substitution::parse_substitution;

        pub(crate) rule inlines(offset: usize, block_metadata: &BlockParsingMetadata) -> Vec<InlineNode>
        = (non_plain_text(offset, block_metadata, true) / plain_text(offset, block_metadata, true))+

        pub(crate) rule inlines_no_autolinks(offset: usize, block_metadata: &BlockParsingMetadata) -> Vec<InlineNode>
        = (non_plain_text(offset, block_metadata, false) / plain_text(offset, block_metadata, false))+

        /// Reduced inline rule set for "quotes" substitution in passthroughs.
        /// Only matches formatting markup + escaped markup + plain text.
        /// Does not match macros, xrefs, anchors, autolinks, footnotes, etc.
        pub(crate) rule quotes_only_inlines(offset: usize, block_metadata: &BlockParsingMetadata) -> Vec<InlineNode>
        = (quotes_non_plain_text(offset, block_metadata) / quotes_plain_text(offset, block_metadata))+

        /// Non-plain-text alternatives for quotes-only mode: formatting markup only.
        /// Keep in sync with the formatting entries in `non_plain_text` above.
        rule quotes_non_plain_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = inline:(
            escaped_super_sub:escaped_superscript_subscript(offset) { escaped_super_sub }
            / escaped_syntax:escaped_syntax(offset) { escaped_syntax }
            / bold_text_unconstrained:bold_text_unconstrained(offset, block_metadata) { bold_text_unconstrained }
            / bold_text_constrained:bold_text_constrained(offset, block_metadata) { bold_text_constrained }
            / italic_text_unconstrained:italic_text_unconstrained(offset, block_metadata) { italic_text_unconstrained }
            / italic_text_constrained:italic_text_constrained(offset, block_metadata) { italic_text_constrained }
            / monospace_text_unconstrained:monospace_text_unconstrained(offset, block_metadata) { monospace_text_unconstrained }
            / monospace_text_constrained:monospace_text_constrained(offset, block_metadata) { monospace_text_constrained }
            / highlight_text_unconstrained:highlight_text_unconstrained(offset, block_metadata) { highlight_text_unconstrained }
            / highlight_text_constrained:highlight_text_constrained(offset, block_metadata) { highlight_text_constrained }
            / superscript_text:superscript_text(offset, block_metadata) { superscript_text }
            / subscript_text:subscript_text(offset, block_metadata) { subscript_text }
            / curved_quotation_text:curved_quotation_text(offset, block_metadata) { curved_quotation_text }
            / curved_apostrophe_text:curved_apostrophe_text(offset, block_metadata) { curved_apostrophe_text }
            / standalone_curved_apostrophe:standalone_curved_apostrophe(offset, block_metadata) { standalone_curved_apostrophe }
        ) {
            inline
        }

        /// Plain text for quotes-only mode: reduced negative lookaheads (formatting patterns only).
        /// Keep in sync with the formatting lookaheads in `plain_text` below.
        rule quotes_plain_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start_pos:position!()
        content:$((
            "\\" "^" !([^'^' | ' ' | '\t' | '\n']+ "^")
            / "\\" "~" !([^'~' | ' ' | '\t' | '\n']+ "~")
            // Fast path: characters that can never start any quotes inline construct.
            // Fewer triggers than plain_text since quotes context has no macros/autolinks.
            / [^('\n' | '\r' | '\\' | '[' | '*' | '_' | '`' | '#' | '^' | '~' | '"' | '\'')]+
            / (
                !(
                    eol()*<2,>
                    / ![_]
                    / &['\\'] escaped_syntax_match()
                    / &['*' | '_' | '`' | '#' | '^' | '~' | '"' | '\'' | '['] (
                        bold_text_unconstrained(start_pos, block_metadata) / bold_text_constrained_match() / italic_text_unconstrained(start_pos, block_metadata) / italic_text_constrained_match() / monospace_text_unconstrained(start_pos, block_metadata) / monospace_text_constrained_match() / highlight_text_unconstrained(start_pos, block_metadata) / highlight_text_constrained_match() / superscript_text(start_pos, block_metadata) / subscript_text(start_pos, block_metadata) / curved_quotation_text(start_pos, block_metadata) / curved_apostrophe_text(start_pos, block_metadata) / standalone_curved_apostrophe(start_pos, block_metadata)
                    )
                )
                [_]
            )
        )+)
        end:position!()
        {
            tracing::info!(?content, "Found quotes-only plain text inline");
            InlineNode::PlainText(Plain {
                content: content.to_string(),
                location: state.create_block_location(start_pos, end, offset),
                escaped: false,
            })
        }

        rule non_plain_text(offset: usize, block_metadata: &BlockParsingMetadata, allow_autolinks: bool) -> InlineNode
        = inline:(
            // Escaped superscript/subscript must come first - produces RawText to prevent re-parsing
            escaped_super_sub:escaped_superscript_subscript(offset) { escaped_super_sub }
            // Escaped syntax must come next - backslash prevents any following syntax from being parsed
            / escaped_syntax:escaped_syntax(offset) { escaped_syntax }
            // Index terms: concealed (triple parens) must come before flow (double parens)
            / check_macros(block_metadata) index_term:index_term_concealed(offset) { index_term }
            / check_macros(block_metadata) index_term:index_term_flow(offset) { index_term }
            / check_macros(block_metadata) indexterm:indexterm_macro(offset) { indexterm }
            / check_macros(block_metadata) indexterm2:indexterm2_macro(offset) { indexterm2 }
            // Bibliography anchor (triple brackets) must come before inline anchor (double brackets)
            / check_macros(block_metadata) bibliography_anchor:bibliography_anchor(offset) { bibliography_anchor }
            / check_macros(block_metadata) inline_anchor:inline_anchor(offset) { inline_anchor }
            / check_macros(block_metadata) cross_reference_shorthand:cross_reference_shorthand(offset, block_metadata) { cross_reference_shorthand }
            / check_macros(block_metadata) cross_reference_macro:cross_reference_macro(offset, block_metadata) { cross_reference_macro }
            / hard_wrap:hard_wrap(offset) { hard_wrap }
            / check_macros(block_metadata) &"footnote:" footnote:footnote(offset, block_metadata) { footnote }
            / check_macros(block_metadata) stem:inline_stem(offset) { stem }
            / check_macros(block_metadata) image:inline_image(offset, block_metadata) { image }
            / check_macros(block_metadata) icon:inline_icon(offset, block_metadata) { icon }
            / check_macros(block_metadata) keyboard:inline_keyboard(offset) { keyboard }
            / check_macros(block_metadata) button:inline_button(offset) { button }
            / check_macros(block_metadata) menu:inline_menu(offset) { menu }
            // mailto has to come before the url_macro because url_macro calls url() which
            // also matches against mailto:
            / check_macros(block_metadata) mailto_macro:mailto_macro(offset, block_metadata) { mailto_macro }
            / check_macros(block_metadata) url_macro:url_macro(offset, block_metadata) { url_macro }
            / check_macros(block_metadata) pass:inline_pass(offset) { pass }
            / check_macros(block_metadata) link_macro:link_macro(offset) { link_macro }
            / check_macros(block_metadata) check_autolinks(allow_autolinks) inline_autolink:inline_autolink(offset) { inline_autolink }
            / inline_line_break:inline_line_break(offset) { inline_line_break }
            / bold_text_unconstrained:bold_text_unconstrained(offset, block_metadata) { bold_text_unconstrained }
            / bold_text_constrained:bold_text_constrained(offset, block_metadata) { bold_text_constrained }
            / italic_text_unconstrained:italic_text_unconstrained(offset, block_metadata) { italic_text_unconstrained }
            / italic_text_constrained:italic_text_constrained(offset, block_metadata) { italic_text_constrained }
            / monospace_text_unconstrained:monospace_text_unconstrained(offset, block_metadata) { monospace_text_unconstrained }
            / monospace_text_constrained:monospace_text_constrained(offset, block_metadata) { monospace_text_constrained }
            / highlight_text_unconstrained:highlight_text_unconstrained(offset, block_metadata) { highlight_text_unconstrained }
            / highlight_text_constrained:highlight_text_constrained(offset, block_metadata) { highlight_text_constrained }
            / superscript_text:superscript_text(offset, block_metadata) { superscript_text }
            / subscript_text:subscript_text(offset, block_metadata) { subscript_text }
            / curved_quotation_text:curved_quotation_text(offset, block_metadata) { curved_quotation_text }
            / curved_apostrophe_text:curved_apostrophe_text(offset, block_metadata) { curved_apostrophe_text }
            / standalone_curved_apostrophe:standalone_curved_apostrophe(offset, block_metadata) { standalone_curved_apostrophe }
            ) {
                inline
            }

        /// Escaped superscript/subscript rule - matches \^content^ or \~content~.
        ///
        /// Produces PlainText with empty substitutions so the content:
        /// 1. Gets HTML escaped by the converter (security)
        /// 2. Doesn't get re-parsed as formatting (no Quotes in substitutions)
        ///
        /// Only matches when there's a complete pattern (content with no spaces
        /// followed by closing marker).
        rule escaped_superscript_subscript(offset: usize) -> InlineNode
        = start:position!() "\\" content:escaped_super_sub_pattern() end:position!() {
            InlineNode::PlainText(Plain {
                content: content.to_string(),
                location: state.create_location(start + offset, end + offset),
                escaped: true,
            })
        }

        /// Match escaped superscript (^content^) or subscript (~content~) pattern.
        rule escaped_super_sub_pattern() -> &'input str
        = $("^" [^'^' | ' ' | '\t' | '\n']+ "^")
        / $("~" [^'~' | ' ' | '\t' | '\n']+ "~")

        /// Generic escaped syntax rule - matches backslash followed by content.
        ///
        /// Handles paired delimiters (`<<...>>`, `[...]`) as complete units,
        /// and simple content until stop characters (space, punctuation).
        rule escaped_syntax(offset: usize) -> InlineNode
        = start:position!() "\\" content:escaped_content() end:position!() {
            InlineNode::PlainText(Plain {
                content: content.to_string(),
                location: state.create_location(start + offset, end + offset),
                escaped: false,
            })
        }

        /// Content after backslash - matches only escapable patterns.
        ///
        /// Matches in order:
        /// 1. Double backslash + escapable pattern (for `\\**`, `\\<<id>>`, etc.)
        /// 2. Paired delimiters: `<<...>>`, `[[...]]`, `prefix[...]`, `{...}`, `((...))`
        /// 3. Typography patterns: `...`, `->`, `<-`, `=>`, `<=`, `--`
        /// 4. Unconstrained formatting markers: `**`, `__`, `##`, ` `` `
        /// 5. Single escapable characters: `*`, `_`, `#`, `` ` ``, `^`, `~`, `[`, `]`, `(`, `&`
        ///
        /// NOTE: Unlike before, this does NOT match arbitrary text. If the backslash is not
        /// followed by something escapable, the rule fails and the backslash flows through
        /// as literal text. This ensures `\\hello` produces `\\hello` (matching asciidoctor).
        rule escaped_content() -> &'input str
        =
        // Double backslash followed by escapable pattern: \\<thing> -> <thing>
        "\\" inner:escapable_pattern() { inner }
        // Single backslash case: \<thing> -> <thing>
        / escapable_pattern()

        /// Patterns that can be escaped with a backslash.
        rule escapable_pattern() -> &'input str
        =
        // Paired angle brackets (cross-refs): <<...>>
        $("<<" (!">>" [_])* ">>")
        // Double square brackets (anchors): [[...]]
        / $("[[" (!"]]" [_])* "]]")
        // Paired square brackets with prefix (macros): something[...]
        / $([^('[' | ' ' | '\t' | '\n' | '\\')]+ "[" [^']']* "]")
        // Curly braces (attributes): {...}
        / $("{" [^'}']* "}")
        // Double parens (index terms): ((...))
        / $("((" (!"))" [_])* "))")
        // Unconstrained formatting: match entire span including content and closing marker
        / $("**" (!"**" [_])* "**")
        / $("__" (!("__" !['_']) [_])* "__")
        / $("``" (!"``" [_])* "``")
        / $("##" (!"##" [_])* "##")
        // Superscript: ^content^ where content has no whitespace
        / $("^" [^'^' | ' ' | '\t' | '\n']+ "^")
        // Subscript: ~content~ where content has no whitespace
        / $("~" [^'~' | ' ' | '\t' | '\n']+ "~")
        // Constrained formatting markers and other single escapable chars
        / $(['*' | '_' | '#' | '`' | '[' | ']' | '&'])
        / $("(" !(("C" / "R" / "TM") ")"))

        /// Match escaped syntax without consuming - for use in negative lookaheads.
        ///
        /// Double backslash + escapable pattern or a single escapable pattern
        rule escaped_syntax_match() -> ()
        = "\\" "\\"? escapable_pattern_match()

        /// Match escapable patterns without consuming
        rule escapable_pattern_match() -> ()
        = "<<" (!">>" [_])* ">>"
        / "[[" (!"]]" [_])* "]]"
        / [^('[' | ' ' | '\t' | '\n' | '\\')]+ "[" [^']']* "]"
        / "{" [^'}']* "}"
        / "((" (!"))" [_])* "))"
        // Unconstrained formatting: match entire span
        / "**" (!"**" [_])* "**"
        / "__" (!("__" !['_']) [_])* "__"
        / "``" (!"``" [_])* "``"
        / "##" (!"##" [_])* "##"
        // Typography patterns handled by converter, not here (see escapable_pattern)
        // Superscript/subscript: require complete pattern
        / "^" [^'^' | ' ' | '\t' | '\n']+ "^"
        / "~" [^'~' | ' ' | '\t' | '\n']+ "~"
        // Single escapable chars (excluding ^ and ~ which need complete patterns)
        / ['*' | '_' | '#' | '`' | '[' | ']' | '&'] {}
        / "(" !(("C" / "R" / "TM") ")")

        rule footnote(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = footnote_match:footnote_match(offset, block_metadata)
        {?
            let (start, id, content_start, content_str, end) = footnote_match;

            tracing::info!(?id, content = %content_str, "Found footnote inline");

            // If content_str is empty or only whitespace, we should not try to process
            // inlines, it just means this footnote has no content and therefore the user
            // has already added the content in a footnote with the same id but with
            // content.
            let content = if content_str.trim().is_empty() {
                vec![]
            } else {
                process_inlines_or_err!(
                    process_inlines(state, block_metadata, &content_start, end, offset, &content_str),
                    "could not process footnote content"
                )?
            };

            let mut footnote = Footnote {
                id: id.clone(),
                content,
                number: 0, // Will be set by register_footnote
                location: state.create_block_location(start, end, offset),
            };
            state.footnote_tracker.push(&mut footnote);

            Ok(InlineNode::Macro(InlineMacro::Footnote(footnote)))
        }

        rule footnote_match(offset: usize, block_metadata: &BlockParsingMetadata) -> (usize, Option<String>, PositionWithOffset, String, usize)
        = start:position!()
        "footnote:"
        // TODO(nlopes): we should change this so that we require an id if content is empty
        id:id()? "[" content_start:position() content:balanced_bracket_content() "]"
        end:position!()
        {
            (start, id.map(String::from), content_start, content.clone(), end)

        }

        /// Parse content that may contain balanced square brackets (general case)
        /// This is used for footnotes, link titles and button labels
        rule balanced_bracket_content() -> String
        = content:$(balanced_bracket_content_part()*) { content.to_string() }

        /// Individual parts of balanced bracket content - either regular text or nested brackets
        rule balanced_bracket_content_part() -> String
        = nested_brackets:("[" inner:balanced_bracket_content() "]" { format!("[{inner}]") })
        / regular_text:$([^('[' | ']')]+) { regular_text.to_string() }

        /// Parse content within brackets using escape handling (no bracket balancing).
        /// Used for stem macros where `\]` and `\[` are common (math notation).
        /// - `\\` → literal `\`
        /// - `\[` or `\]` → literal bracket (backslash stripped)
        /// - `\` before other chars → preserved
        /// - `[` → regular text (no nesting)
        /// - `]` → ends content (consumed by caller)
        rule escaped_bracket_content() -> String
        = parts:escaped_bracket_content_part()* { parts.join("") }

        rule escaped_bracket_content_part() -> &'input str
        = "\\\\" { "\\" }
        / "\\" c:$(['[' | ']']) { c }
        / s:$([^(']' | '\\')]+) { s }
        / "\\" { "\\" }

        /// Parse link/URL title content that may contain balanced brackets
        ///
        /// This is similar to balanced_bracket_content but stops at comma and attribute
        /// patterns
        ///
        /// Supports two formats:
        /// 1. **Quoted text**: `"any text including 'quotes' and ,commas"`
        /// 2. **Unquoted text**: `any text until , or ] or name=value`
        ///
        /// Unlike block attributes, link titles can contain:
        /// - Single quotes: `link:file[see the 'source' code]`
        /// - Periods: `link:file[version 1.2.3 notes]`
        /// - Hash symbols: `link:file[C# programming guide]`
        /// - Other special characters that would terminate block attribute parsing
        ///
        /// The unquoted parsing stops at:
        /// - `,` (start of attributes)
        /// - `]` (end of link)
        /// - `name=` patterns (attribute definitions)
        rule link_title() -> String
        = "\"" title:$((!"\"" [_])*) "\"" { title.to_string() }
        / "'" title:$((!("'" whitespace()* ("," / "]")) [_])*) "'" { title.to_string() }
        / parts:$(balanced_link_title_part()+) { parts.to_string() }

        /// Parse parts of link title content
        rule balanced_link_title_part() -> String
        = nested_brackets:("[" inner:balanced_bracket_content() "]" { format!("[{inner}]") })
        / regular_text:$((!("," whitespace()* (attribute_name() "=" / "]")) [^'[' | ']'])+) { regular_text.to_string() }

        rule inline_pass(offset: usize) -> InlineNode
        = start:position!()
        "pass:"
        substitutions:($([^(']' | ',')]+) ** comma())
        "["
        content:$([^']']+)
        "]"
        end:position!()
        {
            tracing::info!(?content, "Found pass inline");
            InlineNode::Macro(InlineMacro::Pass(Pass {
                text: Some(content.trim().to_string()),
                substitutions: substitutions.into_iter().filter_map(|s| parse_substitution(s.trim())).collect(),
                location: state.create_block_location(start, end, offset),
                kind: PassthroughKind::Macro,
            }))
        }

        /// Concealed index term: (((primary, secondary, tertiary)))
        /// Only appears in the index, not in the text.
        /// Supports hierarchical entries with up to three levels.
        rule index_term_concealed(offset: usize) -> InlineNode
        = start:position!()
        "((("
        terms:index_term_list()
        ")))"
        end:position!()
        {
            let mut iter = terms.into_iter();
            let term = iter.next().unwrap_or_default();
            let secondary = iter.next();
            let tertiary = iter.next();
            tracing::info!(%term, ?secondary, ?tertiary, "Found concealed index term");
            InlineNode::Macro(InlineMacro::IndexTerm(IndexTerm {
                kind: IndexTermKind::Concealed { term, secondary, tertiary },
                location: state.create_block_location(start, end, offset),
            }))
        }

        /// Flow index term: ((term))
        /// Appears both in the text and in the index.
        /// Only supports a primary term.
        rule index_term_flow(offset: usize) -> InlineNode
        = start:position!()
        "(("
        !("(")  // Ensure this is not the start of a concealed term
        term:$((!"))" [_])+)
        "))"
        end:position!()
        {
            let term = term.trim().to_string();
            tracing::info!(%term, "Found flow index term");
            InlineNode::Macro(InlineMacro::IndexTerm(IndexTerm {
                kind: IndexTermKind::Flow(term),
                location: state.create_block_location(start, end, offset),
            }))
        }

        /// indexterm macro: indexterm:[primary, secondary, tertiary]
        /// Concealed (hidden) form - same as (((primary, secondary, tertiary)))
        rule indexterm_macro(offset: usize) -> InlineNode
        = start:position!()
        "indexterm:["
        terms:index_term_list()
        "]"
        end:position!()
        {
            let mut iter = terms.into_iter();
            let term = iter.next().unwrap_or_default();
            let secondary = iter.next();
            let tertiary = iter.next();
            tracing::info!(%term, ?secondary, ?tertiary, "Found indexterm macro");
            InlineNode::Macro(InlineMacro::IndexTerm(IndexTerm {
                kind: IndexTermKind::Concealed { term, secondary, tertiary },
                location: state.create_block_location(start, end, offset),
            }))
        }

        /// indexterm2 macro: indexterm2:[term]
        /// Flow (visible) form - same as ((term))
        rule indexterm2_macro(offset: usize) -> InlineNode
        = start:position!()
        "indexterm2:["
        term:$([^']']+)
        "]"
        end:position!()
        {
            let term = term.trim().to_string();
            tracing::info!(%term, "Found indexterm2 macro");
            InlineNode::Macro(InlineMacro::IndexTerm(IndexTerm {
                kind: IndexTermKind::Flow(term),
                location: state.create_block_location(start, end, offset),
            }))
        }

        /// Parse comma-separated index term list with support for quoted segments
        /// e.g., "knight, Knight of the Round Table, Lancelot"
        /// or "knight, \"Arthur, King\"" (quoted segment with embedded comma)
        rule index_term_list() -> Vec<String>
        = terms:(index_term_segment() ** ",") {
            terms.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        }

        /// Parse a single index term segment, either quoted or unquoted
        rule index_term_segment() -> String
        = whitespace()? segment:(index_term_quoted() / index_term_unquoted()) whitespace()? { segment }

        /// Quoted segment: "term with, comma"
        rule index_term_quoted() -> String
        = "\"" content:$([^'"']*) "\"" { content.to_string() }

        /// Unquoted segment: term without comma
        rule index_term_unquoted() -> String
        = content:$([^('"' | ',' | ')' | ']')]+) { content.to_string() }

        /// Match index term patterns without consuming (for negative lookahead in plain_text)
        rule index_term_match() -> ()
        = "(((" (!"))" [_])* ")))"  // Concealed: (((term)))
        / "((" !("(") (!"))" [_])+ "))"  // Flow: ((term))
        / "indexterm:[" [^']']* "]"  // indexterm:[term]
        / "indexterm2:[" [^']']* "]"  // indexterm2:[term]

        rule inline_menu(offset: usize) -> InlineNode
        = start:position!()
        "menu:"
        target:$([^'[']+)
        "["
        items:((item:$([^(']' | '>')]+) { item.trim().to_string() }) ** (">" whitespace()?))
        "]"
        end:position!()
        {
            tracing::info!(%target, ?items, "Found menu inline");
            InlineNode::Macro(InlineMacro::Menu(Menu {
                target: target.to_string(),
                items,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule inline_button(offset: usize) -> InlineNode
        = start:position!()
        "btn:[" label:$balanced_bracket_content() "]" end:position!()
        {
            tracing::info!(?label, "Found button inline");
            InlineNode::Macro(InlineMacro::Button(Button {
                label: label.trim().to_string(),
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule inline_keyboard(offset: usize) -> InlineNode
        = start:position!()
        "kbd:["
        keys:((key:$([^(']' | '+' | ',')]+) { key.trim().to_string() }) ** (("," / "+") whitespace()?))
        "]"
        end:position!()
        {
            tracing::info!(?keys, "Found keyboard inline");
            InlineNode::Macro(InlineMacro::Keyboard(Keyboard {
                keys,
                location: state.create_block_location(start, end, offset),
            }))
        }

        /// Parse URL macros with attribute handling.
        ///
        /// URL macros have the format: `https://example.com[text,attr1=value1,attr2=value2]`
        ///
        /// This is similar to link macros but the URL is directly specified rather than
        /// using the `link:` prefix.
        rule url_macro(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position()
        target:url()
        "["
        content_start:position()
        content:(
            title:link_title() attributes:("," att:attribute() { att })* {
                (Some(title), attributes.into_iter().flatten().collect::<Vec<_>>())
            } /
            attributes:(att:attribute() comma()? { att })* {
                (None, attributes.into_iter().flatten().collect::<Vec<_>>())
            }
        )
        "]"
        end:position!()
        {?
            tracing::info!(?target, "Found url macro");
            let (text, attributes) = content;
            let mut metadata = BlockMetadata::default();
            for (k, v, _pos) in attributes {
                if let AttributeValue::String(v) = v {
                    metadata.attributes.insert(k, AttributeValue::String(v));
                }
            }
            let text = if let Some(text) = text {
                process_inlines_no_autolinks(state, block_metadata, &content_start, end, offset, &text)
                    .map_err(|e| {
                        tracing::error!(?e, url_text = text, "could not process URL macro text");
                        "could not process URL macro text"
                    })?
            } else {
                vec![]
            };
            let target_source = Source::from_str(&target).map_err(|_| "failed to parse URL target")?;
            Ok(InlineNode::Macro(InlineMacro::Url(Url {
                text,
                target: target_source,
                attributes: metadata.attributes.clone(),
                location: state.create_block_location(start.offset, end, offset),
            })))
        }

        /// Parse `mailto:` macros with attribute handling.
        ///
        /// `mailto:` macros have the format: `mailto:joe@example.com[text,attr1=value1,attr2=value2]`
        ///
        /// This is similar to link macros but the `mailto:` is directly specified rather
        /// than using the `link:` prefix.
        rule mailto_macro(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position()
        &"mailto:"
        target:url()
        "["
        content_start:position()
        content:(
            title:link_title() attributes:("," att:attribute() { att })* {
                (Some(title), attributes.into_iter().flatten().collect::<Vec<_>>())
            } /
            attributes:(att:attribute() comma()? { att })* {
                (None, attributes.into_iter().flatten().collect::<Vec<_>>())
            }
        )
        "]"
        end:position!()
        {?
            tracing::info!(?target, "Found mailto macro");
            let (text, attributes) = content;
            let mut metadata = BlockMetadata::default();
            for (k, v, _pos) in attributes {
                if let AttributeValue::String(v) = v {
                    metadata.attributes.insert(k, AttributeValue::String(v));
                }
            }
            let text = if let Some(text) = text {
                process_inlines_no_autolinks(state, block_metadata, &content_start, end, offset, &text)
                    .map_err(|e| {
                        tracing::error!(?e, url_text = text, "could not process mailto macro text");
                        "could not process mailto macro text"
                    })?
            } else {
                vec![]
            };
            let target_source = Source::from_str(&target).map_err(|_| "failed to parse mailto target")?;
            Ok(InlineNode::Macro(InlineMacro::Mailto(Mailto {
                text,
                target: target_source,
                attributes: metadata.attributes.clone(),
                location: state.create_block_location(start.offset, end, offset),
            })))
        }

        rule check_autolinks(allow: bool) -> ()
        = {? if allow { Ok(()) } else { Err("autolinks suppressed") } }

        rule check_macros(block_metadata: &BlockParsingMetadata) -> ()
        = {? if block_metadata.macros_enabled { Ok(()) } else { Err("macros disabled") } }

        rule inline_autolink(offset: usize) -> InlineNode
        =
        start:position!()
        url_info:(
            "<" url:url() ">" { (url, true) }
            / "<" url:email_address() ">" { (format!("mailto:{url}"), true) }
            / url:bare_url() { (url, false) }
            / url:email_address() { (format!("mailto:{url}"), false) }
        )
        end:position!()
        {?
            let (url, bracketed) = url_info;
            tracing::info!(?url, bracketed, "Found autolink inline");
            let url_source = Source::from_str(&url).map_err(|_| "failed to parse autolink URL")?;
            Ok(InlineNode::Macro(InlineMacro::Autolink(Autolink {
                url: url_source,
                bracketed,
                location: state.create_block_location(start, end, offset),
            })))
        }

        rule inline_line_break(offset: usize) -> InlineNode
        = start:position!() " +" end:position!() eol()
        {?
            // Hard line break requires `text +` where text is actual content (non-whitespace)
            // When `+` appears indented at the start of a line (after newline + whitespace),
            // it should be treated as literal text, not a hard break.
            // See: https://github.com/nlopes/acdc/issues/234
            let absolute_pos = start + offset;
            let valid = absolute_pos > 0 && {
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_some_and(|&b| {
                    !b.is_ascii_whitespace()
                })
            };

            if !valid {
                return Err("hard line break requires preceding non-whitespace");
            }

            tracing::info!("Found inline line break");
            Ok(InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule hard_wrap(offset: usize) -> InlineNode
            = start:position!() " + \\" end:position!() &eol()
        {
            tracing::info!("Found hard wrap inline");
            InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(start, end, offset),
            })
        }

        rule inline_icon(offset: usize, _block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position() "icon:" source:source() attributes:macro_attributes() end:position!()
        {
            let (_discrete, metadata, _title_position) = attributes;
            let mut metadata = metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            // For font mode, the first positional (style) can be a size value (1x, 2x,
            // lg, fw) -> stored as "size" attribute;
            //
            // For image mode, the first positional (style) can be alt text.
            if let Some(ref style) = metadata.style {
                // Strip surrounding quotes if present (quoted positional attributes)
                let style_value = strip_quotes(style);
                if ICON_SIZES.contains(&style_value) {
                    // Named size= attribute takes precedence over positional size so we
                    // insert rather than set (set overrides).
                    metadata.attributes.insert(
                        "size".to_string(),
                        AttributeValue::String(style_value.to_string()),
                    );
                } else {
                    // Other value become alt (fa-{value} in image mode)
                    metadata.attributes.set(
                        "alt".to_string(),
                        AttributeValue::String(style_value.to_string()),
                    );
                }
            }
            // Copy roles to attributes so they're accessible in the converter
            if !metadata.roles.is_empty() {
                metadata.attributes.set(
                    "role".to_string(),
                    AttributeValue::String(metadata.roles.join(" ")),
                );
            }
            InlineNode::Macro(InlineMacro::Icon(Icon {
                target: source,
                attributes: metadata.attributes.clone(),
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule inline_stem(offset: usize) -> InlineNode
        = start:position!() prefix:$("latexmath" / "asciimath" / "stem") ":[" content:escaped_bracket_content() "]" end:position!()
        {
            let notation = match prefix {
                "latexmath" => StemNotation::Latexmath,
                "asciimath" => StemNotation::Asciimath,
                _ => {
                    // stem:[] — resolve from :stem: document attribute
                    match state.document_attributes.get_string("stem") {
                        Some(s) => StemNotation::from_str(&s).unwrap_or(StemNotation::Asciimath),
                        _ => StemNotation::Asciimath,
                    }
                }
            };

            InlineNode::Macro(InlineMacro::Stem(Stem {
                content,
                notation,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule inline_image(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position() "image:" source:source() attributes:macro_attributes() end:position!()
        {?
            let (_discrete, metadata, title_position) = attributes;
            let mut metadata = metadata.clone();
            let mut title = crate::Title::default();
            if let Some(style) = metadata.style {
                metadata.style = None; // Clear style to avoid confusion
                // For inline images, the first positional attribute is the alt text (title)
                title = crate::Title::new(vec![InlineNode::PlainText(Plain {
                    content: style,
                    location: state.create_block_location(start.offset, end, offset),
                    escaped: false,
                })]);
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".into(), AttributeValue::String(metadata.positional_attributes.remove(1)));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("width".into(), AttributeValue::String(metadata.positional_attributes.remove(0)));
            }
            metadata.move_positional_attributes_to_attributes();
            // For inline images, if there's no first positional (no alt text in title field),
            // check if there's a named title attribute. Only then should we use it to populate
            // the title field for rendering purposes, but we keep it in attributes for the
            // HTML title attribute (hover text).
            if title.is_empty()
                && let Some(AttributeValue::String(content)) = metadata.attributes.get("title")
                && let Some((title_start, title_end)) = title_position
            {
                // Use the captured position from the named_attribute rule
                let title_start_pos = PositionWithOffset {
                    offset: title_start,
                    position: state.line_map.offset_to_position(title_start, &state.input),
                };
                title = crate::Title::new(process_inlines_or_err!(
                    process_inlines(state, block_metadata, &title_start_pos, title_end, offset, content),
                    "could not process title in inline image macro"
                )?);
            }
            // Note: We do NOT remove the title attribute - it's needed for the HTML title attribute

            Ok(InlineNode::Macro(InlineMacro::Image(Box::new(Image {
                title,
                source,
                metadata: metadata.clone(),
                location: state.create_block_location(start.offset, end, offset),

            }))))
        }

        /// Parse link macros with custom attribute handling.
        ///
        /// Link macros have the format: `link:target[text,attr1=value1,attr2=value2]`
        ///
        /// ## Why Custom Parsing is Required
        ///
        /// Link attributes cannot use the generic `attributes()` rule because:
        ///
        /// 1. **Different Character Rules**: Link text can contain single quotes (`'`) and other
        ///    characters that are treated as delimiters in block attributes. For example:
        ///    - `link:file.adoc[see the 'quoted' text]` - single quotes are valid in link text
        ///
        /// 2. **Text vs Attributes**: The first element in link brackets is display text,
        ///    not an attribute. Block attributes expect all content to be attribute
        ///    definitions or block style.
        ///
        /// 3. **Delimiter Precedence**: In links, commas separate text from attributes, while in
        ///    block attributes, the first positional value is treated as a style/role.
        ///
        /// ## Parsing Strategy
        ///
        /// 1. **Try text + attributes**: `link_title()` followed by comma-separated attributes
        /// 2. **Fallback to attributes only**: If no valid title is found, parse as pure attributes
        ///
        /// The `link_title()` rule handles both quoted (`"text"`) and unquoted text, stopping at:
        /// - Commas (indicating start of attributes)
        /// - Closing brackets (end of link)
        /// - Attribute patterns (`name=value`)
        ///
        /// This approach isolates link parsing from block attribute parsing, preventing
        /// regressions in other parts of the parser while correctly handling edge cases
        /// like quotes, special characters, and mixed content.
        rule link_macro(offset: usize) -> InlineNode
        = start:position!() "link:" target:source() fragment:path_fragment()? "[" content:(
            title:link_title() attributes:("," att:attribute() { att })* {
                (Some(title), attributes.into_iter().flatten().collect::<Vec<_>>())
            } /
            attributes:(att:attribute() comma()? { att })* {
                (None, attributes.into_iter().flatten().collect::<Vec<_>>())
            }
        ) "]" end:position!()
        {?
            tracing::info!(?target, ?content, "Found link macro inline");
            let (text, attributes) = content;
            let mut metadata = BlockMetadata::default();
            for (k, v, _pos) in attributes {
                if let AttributeValue::String(v) = v {
                    metadata.attributes.insert(k, AttributeValue::String(v));
                }
            }
            let target = match fragment {
                Some(f) => Source::from_str(&format!("{target}{f}")).unwrap_or(target),
                None => target,
            };
            Ok(InlineNode::Macro(InlineMacro::Link(Link {
                text,
                target,
                attributes: metadata.attributes.clone(),
                location: state.create_block_location(start, end, offset),
            })))
        }

        /// Parse cross-reference shorthand syntax: <<id>> or <<id,custom text>>
        rule cross_reference_shorthand(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position() shorthand:cross_reference_shorthand_pattern() end:position!()
        {?
            let (target, raw_text) = shorthand;
            let target_str = target.trim().to_string();
            let text = if let Some((content_start, t)) = raw_text {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    vec![]
                } else {
                    let content_pos = PositionWithOffset {
                        offset: content_start,
                        position: state.line_map.offset_to_position(content_start, &state.input),
                    };
                    process_inlines_no_autolinks(state, block_metadata, &content_pos, end, offset, trimmed)
                        .map_err(|e| {
                            tracing::error!(?e, xref_text = trimmed, "could not process xref text");
                            "could not process xref text"
                        })?
                }
            } else {
                vec![]
            };
            tracing::info!(?target_str, ?text, "Found cross-reference shorthand");
            Ok(InlineNode::Macro(InlineMacro::CrossReference(crate::model::CrossReference {
                target: target_str,
                text,
                location: state.create_block_location(start.offset, end, offset),
            })))
        }

        /// Pattern for cross-reference shorthand: <<id>> or <<id,custom text>>
        rule cross_reference_shorthand_pattern() -> (String, Option<(usize, String)>)
        = "<<" target:$(['a'..='z' | 'A'..='Z' | '_'] ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']*) content:("," content_start:position!() text:$((!">>" [_])+) { (content_start, text.to_string()) })? ">>"
        {
            (target.to_string(), content)
        }

        /// Parse cross-reference macro syntax: xref:id[text] or xref:file.adoc#anchor[text]
        rule cross_reference_macro(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = start:position() "xref:" target:source() fragment:path_fragment()? "[" content_start:position() raw_text:$((!"]" [_])*) "]" end:position!()
        {?
            let target_str = match fragment {
                Some(f) => format!("{target}{f}"),
                None => target.to_string(),
            };
            let text = if raw_text.is_empty() {
                vec![]
            } else {
                process_inlines_no_autolinks(state, block_metadata, &content_start, end, offset, raw_text)
                    .map_err(|e| {
                        tracing::error!(?e, xref_text = raw_text, "could not process xref text");
                        "could not process xref text"
                    })?
            };
            tracing::info!(?target_str, ?text, "Found cross-reference macro");
            Ok(InlineNode::Macro(InlineMacro::CrossReference(crate::model::CrossReference {
                target: target_str,
                text,
                location: state.create_block_location(start.offset, end, offset),
            })))
        }

        /// Match cross-reference shorthand syntax without consuming: <<id>> or <<id,text>>
        rule cross_reference_shorthand_match() -> ()
        = "<<" ['a'..='z' | 'A'..='Z' | '_'] ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']* ("," (!">>" [_])+)? ">>"

        /// Match cross-reference macro syntax without consuming: xref:id[text] or xref:file.adoc#anchor[text]
        rule cross_reference_macro_match()
        = "xref:" source() path_fragment()? "[" (!"]" [_])* "]"

        rule bold_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "**" content_start:position() content:$((!(eol() / ![_] / "**") [_])+) "**" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found unconstrained bold text inline");
            let content = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process unconstrained bold text content"
            )?;
            Ok(InlineNode::BoldText(Bold {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule bold_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "*"
        content:$([^(' ' | '\t' | '\n')] [^'*']* ("*" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_]) [^'*']*)*)
        "*"
        end:position!() &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_])
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter) {
                tracing::debug!(absolute_pos, prev_byte = ?state.input.as_bytes().get(absolute_pos.saturating_sub(1)), "Invalid word boundary for constrained bold");
                return Err("invalid word boundary for constrained bold");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained bold");
            }

            tracing::info!(?offset, ?content, ?role, "Found constrained bold text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'*');
            let result = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &adjusted_content_start, end - 1, offset, content),
                "could not process constrained bold text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let content = result?;

            Ok(InlineNode::BoldText(Bold {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule bold_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        "*"
        [^(' ' | '\t' | '\n')]
        [^'*']*
        ("*" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_]) [^'*']*)*
        "*"
        closing_pos:position!()
        ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_])
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter);
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "_"
        content:$([^(' ' | '\t' | '\n')] [^'_']* ("_" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_]) [^'_']*)*)
        "_"
        end:position!() &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_])
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter) {
                return Err("invalid word boundary for constrained italic");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained italic");
            }

            tracing::info!(?offset, ?content, ?role, "Found constrained italic text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'_');
            let result = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &adjusted_content_start, end - 1, offset, content),
                "could not process constrained italic text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let content = result?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule italic_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        "_"
        [^(' ' | '\t' | '\n')]
        [^'_']*
        ("_" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_]) [^'_']*)*
        "_"
        closing_pos:position!()
        ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_])
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter);
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "__" content_start:position() content:$((!(eol() / ![_] / "__") [_])+) "__" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found unconstrained italic text inline");
            let content = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process unconstrained italic text content"
            )?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule monospace_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "``" content_start:position() content:$((!(eol() / ![_] / "``") [_])+) "``" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found unconstrained monospace text inline");
            let content = process_inlines_or_err!(
                process_inlines(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process unconstrained monospace text content"
            )?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule monospace_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "`"
        content:$([^(' ' | '\t' | '\n')] [^'`']* ("`" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_]) [^'`']*)*)
        "`"
        end:position!()
        &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_])
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter) {
                return Err("monospace must be at word boundary");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained monospace");
            }

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found constrained monospace text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'`');
            let result = process_inlines_or_err!(
                process_inlines(state, block_metadata, &adjusted_content_start, end - 1, offset, content),
                "could not process constrained monospace text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let content = result?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule monospace_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        "`"
        [^(' ' | '\t' | '\n')]
        [^'`']*
        ("`" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_]) [^'`']*)*
        "`"
        closing_pos:position!()
        ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_])
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter);
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("monospace must be at word boundary") }
        }

        rule highlight_text_unconstrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "##" content_start:position() content:$((!(eol() / ![_] / "##") [_])+) "##" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found unconstrained highlight text inline");
            let content = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process unconstrained highlight text content"
            )?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule highlight_text_constrained(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "#"
        content:$([^(' ' | '\t' | '\n')] [^'#']* ("#" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_]) [^'#']*)*)
        "#"
        end:position!()
        &([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_])
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter) {
                tracing::debug!(absolute_pos, prev_byte = ?state.input.as_bytes().get(absolute_pos.saturating_sub(1)), "Invalid word boundary for constrained highlight");
                return Err("invalid word boundary for constrained highlight");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained highlight");
            }

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found constrained highlight text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'#');
            let result = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &adjusted_content_start, end - 1, offset, content),
                "could not process constrained highlight text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let content = result?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule highlight_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        "#"
        [^(' ' | '\t' | '\n')]
        [^'#']*
        ("#" !([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_]) [^'#']*)*
        "#"
        closing_pos:position!()
        ([' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '-' | '|' | '<' | '>' | '^' | '~'] / ![_])
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter);
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("invalid word boundary") }
        }

        /// Parse superscript text (^text^)
        rule superscript_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "^" content_start:position() content:$([^('^' | ' ' | '\t' | '\n')]+) "^" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found superscript text inline");
            let content = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &content_start, end - 1, offset, content),
                "could not process superscript text content"
            )?;
            Ok(InlineNode::SuperscriptText(Superscript {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        /// Parse subscript text (~text~)
        rule subscript_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "~" content_start:position() content:$([^('~' | ' ' | '\t' | '\n')]+) "~" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found subscript text inline");
            let content = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &content_start, end - 1, offset, content),
                "could not process subscript text content"
            )?;
            Ok(InlineNode::SubscriptText(Subscript {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        /// Parse curved quotation text (`"text"`)
        rule curved_quotation_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "\"`" content_start:position() content:$((!("`\"") [_])+) "`\"" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found curved quotation text inline");
            let content = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process curved quotation text content"
            )?;
            Ok(InlineNode::CurvedQuotationText(CurvedQuotation {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        /// Parse curved apostrophe text (`'text'`)
        rule curved_apostrophe_text(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = attrs:inline_attributes()? start:position() "'`" content_start:position() content:$((!("`'") [_])+) "`'" end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(roles.join(" "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| id.clone());

            tracing::info!(?start, ?content_start, ?end, ?offset, ?content, ?role, "Found curved apostrophe text inline");
            let content = process_inlines_or_err!(
                process_inlines_preprocessed(state, block_metadata, &content_start, end - 2, offset, content),
                "could not process curved apostrophe text content"
            )?;
            Ok(InlineNode::CurvedApostropheText(CurvedApostrophe {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        /// Parse standalone curved apostrophe (`')
        rule standalone_curved_apostrophe(offset: usize, block_metadata: &BlockParsingMetadata) -> InlineNode
            = start:position() "`'" end:position!()
        {?
            tracing::info!(?start, ?end, ?offset, "Found standalone curved apostrophe inline");
            Ok(InlineNode::StandaloneCurvedApostrophe(StandaloneCurvedApostrophe {
                location: state.create_block_location(start.offset, end, offset),
            }))
        }

        rule warn_anchor_id_with_whitespace() -> ()
        = start:position!()
        &(
            id:$([^'\'' | ',' | ']' | '.' | '#']+)
            end:position!()
            {?
                if id.chars().any(char::is_whitespace) {
                    state.add_warning(format!("anchor id '{id}' contains whitespace which is not allowed, treating as literal text"));
                }
                // Always fail so the lookahead doesn't match - we just want the side
                // effect
                Err::<(), &'static str>("")
            }
        )

        rule inline_anchor(offset: usize) -> InlineNode
        = start:position!()
        double_open_square_bracket()
        // Whitespace is excluded - IDs must not contain spaces
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '.' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(
            comma() reftext:$([^']']+) {
                Some(reftext)
            } /
            {
                None
            }
        )
        double_close_square_bracket()
        end:position!()
        {
            let substituted_id = substitute(id, HEADER, &state.document_attributes);
            let substituted_reftext = reftext.map(|rt| substitute(rt, HEADER, &state.document_attributes));
            InlineNode::InlineAnchor(Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_block_location(start, end, offset)
            })
        }

        rule inline_anchor_match() -> ()
        = double_open_square_bracket() [^'\'' | ',' | ']' | '.' | ' ' | '\t' | '\n' | '\r']+ (comma() [^']']+)? double_close_square_bracket()

        /// Bibliography anchor: `[[[id]]]` or `[[[id,reftext]]]`
        /// Must be parsed before inline_anchor to avoid capturing `[id` as the ID
        rule bibliography_anchor(offset: usize) -> InlineNode
        = start:position!()
        "[[["
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '[' | '.' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(comma() reftext:$([^']']+) { Some(reftext) } / { None })
        "]]]"
        end:position!()
        {
            let substituted_id = substitute(id, HEADER, &state.document_attributes);
            let substituted_reftext = reftext.map(|rt| substitute(rt, HEADER, &state.document_attributes));
            InlineNode::InlineAnchor(Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_block_location(start, end, offset)
            })
        }

        rule plain_text(offset: usize, block_metadata: &BlockParsingMetadata, allow_autolinks: bool) -> InlineNode
        = start_pos:position!()
        content:$((
            // Escape sequences for superscript/subscript markers - only when NOT followed by
            // a complete pattern (those are handled by escaped_superscript_subscript rule)
            "\\" "^" !([^'^' | ' ' | '\t' | '\n']+ "^")
            / "\\" "~" !([^'~' | ' ' | '\t' | '\n']+ "~")
            // Fast path: characters that can never start any inline construct.
            // Conservative set — excludes all alphanumerics (bare email autolinks can start
            // with any letter/digit), formatting markers, escape chars, and construct openers.
            // Safe: tab, common punctuation like , ; . ? ! : - / > ) ] } | @ & = {
            / ['\t' | ',' | ';' | '.' | '?' | '!' | ':' | '/' | '>' | ')' | ']' | '}' | '|' | '@' | '&' | '=' | '{' | '\u{00A0}'..='\u{10FFFF}']+
            // Slow path: potential construct trigger character. Use character-class guards to
            // skip groups of rules whose starting character doesn't match.
            / (
                !(
                    eol()*<2,>
                    / ![_]
                    / &['\\'] escaped_syntax_match()
                    / &[' '] (hard_wrap(offset) / inline_line_break(start_pos))
                    // Macro guard: [ ( < for delimiters, then first letters of each macro:
                    // a=asciimath, b=btn, f=footnote/ftp, h=http(s), i=image/icon/indexterm/irc,
                    // k=kbd, l=link/latexmath, m=menu/mailto, p=pass, s=stem, x=xref
                    / (check_macros(block_metadata) &['[' | '(' | '<' | 'a' | 'b' | 'f' | 'h' | 'i' | 'k' | 'l' | 'm' | 'p' | 's' | 'x'] (inline_anchor_match() / index_term_match() / cross_reference_shorthand_match() / cross_reference_macro_match() / footnote_match(offset, block_metadata) / inline_image(start_pos, block_metadata) / inline_icon(start_pos, block_metadata) / inline_stem(start_pos) / inline_keyboard(start_pos) / inline_button(start_pos) / inline_menu(start_pos) / mailto_macro(start_pos, block_metadata) / url_macro(start_pos, block_metadata) / inline_pass(start_pos) / link_macro(start_pos)))
                    / (check_macros(block_metadata) check_autolinks(allow_autolinks) inline_autolink(start_pos))
                    / &['*' | '_' | '`' | '#' | '^' | '~' | '"' | '\'' | '['] (bold_text_unconstrained(start_pos, block_metadata) / bold_text_constrained_match() / italic_text_unconstrained(start_pos, block_metadata) / italic_text_constrained_match() / monospace_text_unconstrained(start_pos, block_metadata) / monospace_text_constrained_match() / highlight_text_unconstrained(start_pos, block_metadata) / highlight_text_constrained_match() / superscript_text(start_pos, block_metadata) / subscript_text(start_pos, block_metadata) / curved_quotation_text(start_pos, block_metadata) / curved_apostrophe_text(start_pos, block_metadata) / standalone_curved_apostrophe(start_pos, block_metadata))
                ) [_]
            )
        )+)
        end:position!()
        {
            tracing::info!(?content, "Found plain text inline");
            // Note: Backslash escape stripping (e.g., \^ -> ^) is handled by the converter,
            // not here, so that verbatim contexts (like monospace) preserve backslashes.
            InlineNode::PlainText(Plain {
                content: content.to_string(),
                location: state.create_block_location(start_pos, end, offset),
                escaped: false,
            })
        }

        /// Parse optional attribute list for inline elements
        /// Returns (roles, id) extracted from attributes like [.role1.role2] or [#id.role]
        /// This is a simplified version of block attributes, used for inline formatting
        /// In inline context, % is treated as a literal character, not an option separator
        /// Stops parsing shorthands at invalid characters (comma, space, etc.)
        rule inline_attributes() -> (Vec<String>, Option<String>)
        = open_square_bracket() shorthands:inline_shorthand()+ [^']']* close_square_bracket()
        {
            let mut roles = Vec::new();
            let mut id = None;

            for s in shorthands {
                match s {
                    Shorthand::Role(r) => roles.push(r),
                    Shorthand::Id(i) => {
                        // If multiple IDs are specified, last one wins
                        id = Some(i);
                    }
                    Shorthand::Option(o) => {
                        // Options are not parsed by inline_shorthand, this branch should not occur
                        // Defensive: log and continue rather than panic
                        tracing::error!(option=?o, "inline_shorthand() unexpectedly produced Option variant");
                    }
                }
            }

            (roles, id)
        }

        /// Parse inline attribute shorthand: .role, #id, %role, or bare role
        /// In inline context, % is not an option separator - it's a literal character
        /// Leading % is treated as part of the role name
        /// Bare roles (no prefix) are supported for asciidoctor compatibility
        rule inline_shorthand() -> Shorthand
        = "#" id:inline_id() { Shorthand::Id(id.to_string()) }
        / "." role:inline_role() { Shorthand::Role(role.to_string()) }
        / "%" role:inline_role() { Shorthand::Role(format!("%{role}")) }
        / role:bare_inline_role() { Shorthand::Role(role.to_string()) }

        /// Bare role pattern for inline contexts (no prefix) - matches CSS-like identifiers
        /// Starts with letter, followed by letters, numbers, or hyphens
        /// Used for syntax like [line-through]#text# (asciidoctor compatibility)
        rule bare_inline_role() -> &'input str = $(['a'..='z' | 'A'..='Z'] ['a'..='z' | 'A'..='Z' | '0'..='9' | '-']*)

        /// Role pattern for inline contexts - allows % as literal character
        rule inline_role() -> &'input str = $([^(',' | ']' | '#' | '.')]+)

        /// ID pattern for inline contexts - allows % as literal character
        rule inline_id() -> &'input str = $(id_start_char() inline_id_subsequent_char()*)
        rule inline_id_subsequent_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' | '%']

        /// Macro attribute parsing - simpler than block attributes.
        ///
        /// Does NOT support shorthand syntax (.role, #id, %option).
        /// Shorthands are only valid in block-level attributes, not inside macro brackets.
        ///
        /// Asciidoctor behavior:
        /// - `image::photo.jpg[.role]` -> alt=".role" (literal text, NOT a role)
        /// - `image::photo.jpg[Diablo 4 picture of Lilith.]` -> alt="Diablo 4 picture of Lilith."
        pub(crate) rule macro_attributes() -> (bool, BlockMetadata, Option<(usize, usize)>)
            = start:position!() open_square_bracket()
              attrs:(att:macro_attribute() comma()? { att })*
              close_square_bracket() end:position!()
        {
            let mut metadata = BlockMetadata::default();
            let title_position = process_attribute_list(
                attrs,
                &mut metadata,
                state,
                start,
                end,
                AttributeProcessingMode::MACRO,
            );
            // macro_attributes never sets discrete flag (that's block-level only)
            (false, metadata, title_position)
        }

        /// Positional value in macro attributes - allows . # % as literal characters
        /// This is the key difference from block attributes.
        rule macro_positional_value() -> Option<String>
            = quoted:inner_attribute_value() {
                let trimmed = strip_quotes(&quoted);
                if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
            }
            / s:$([^('"' | ',' | ']' | '=')]+) {
                let trimmed = s.trim();
                if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
            }

        /// Named attribute or additional positional in macro context
        rule macro_attribute() -> Option<(String, AttributeValue, Option<(usize, usize)>)>
            = whitespace()* att:named_attribute() { att }
            / val:macro_positional_value() {
                val.map(|v| (v, AttributeValue::None, None))
            }

        rule open_square_bracket() = "["
        rule close_square_bracket() = "]"
        rule double_open_square_bracket() = "[["
        rule double_close_square_bracket() = "]]"
        rule comma() = ","
        rule period() = "."
        rule empty_style() = ""
        rule role() -> &'input str = $([^(',' | ']' | '#' | '.' | '%')]+)

        rule attribute_name() -> &'input str = $((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+)

        rule attribute() -> Option<(String, AttributeValue, Option<(usize, usize)>)>
            = whitespace()* att:named_attribute() { att }
              / whitespace()* start:position!() att:positional_attribute_value() end:position!() {
                  let substituted = substitute(&att, &[Substitution::Attributes], &state.document_attributes);
                  Some((substituted, AttributeValue::None, Some((start, end))))
              }

        // Add a simple ID rule
        rule id() -> &'input str
            = $((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+)

        rule id_start_char() = ['A'..='Z' | 'a'..='z' | '_']

        rule block_style_id() -> &'input str = $(id_start_char() block_style_id_subsequent_char()*)

        rule block_style_id_subsequent_char() =
            ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-']

        // TODO(nlopes): this should instead return an enum
        rule named_attribute() -> Option<(String, AttributeValue, Option<(usize, usize)>)>
            = "id" "=" start:position!() id:id() end:position!()
                { Some((RESERVED_NAMED_ATTRIBUTE_ID.to_string(), AttributeValue::String(id.to_string()), Some((start, end)))) }
              / ("role" / "roles") "=" value:named_attribute_value()
                { Some((RESERVED_NAMED_ATTRIBUTE_ROLE.to_string(), AttributeValue::String(value), None)) }
              / ("options" / "opts") "=" value:named_attribute_value()
                { Some((RESERVED_NAMED_ATTRIBUTE_OPTIONS.to_string(), AttributeValue::String(value), None)) }
              / name:attribute_name() "=" start:position!() value:named_attribute_value() end:position!()
                {
                    let substituted_value = substitute(&value, &[Substitution::Attributes], &state.document_attributes);
                    Some((name.to_string(), AttributeValue::String(substituted_value), Some((start, end))))
                }

        rule named_attribute_value() -> String
        = &("\"" / "'") inner:inner_attribute_value()
        {
            // Strip surrounding quotes from quoted values
            let trimmed = strip_quotes(&inner);
            tracing::debug!(%inner, %trimmed, "Found named attribute value (inner)");
            trimmed.to_string()
        }
        / s:$([^(',' | '"' | '\'' | ']')]+)
        {
            tracing::debug!(%s, "Found named attribute value");
            s.to_string()
        }

        rule positional_attribute_value() -> String
        = quoted:inner_attribute_value() {
            let trimmed = strip_quotes(&quoted);
            tracing::debug!(%quoted, %trimmed, "Found quoted positional attribute value");
            trimmed.to_string()
        }
        / s:$([^('"' | ',' | ']' | '#' | '.' | '%')] [^(',' | ']' | '#' | '.' | '%' | '=')]*)
        {
            let trimmed = s.trim();
            tracing::debug!(%s, %trimmed, "Found unquoted positional attribute value");
            trimmed.to_string()
        }

        rule inner_attribute_value() -> String
        = s:$("\"" [^'"']* "\"") { s.to_string() }
        / s:$("'" [^'\'']* "'") { s.to_string() }

        /// URL rule matches both web URLs (proto://) and mailto: URLs
        pub rule url() -> String =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:url_path() { format!("{proto}://{path}") }
        / "mailto:" email:email_address() { format!("mailto:{email}") }

        /// Email address pattern (RFC 822 simplified)
        ///
        /// Local part: alphanumeric plus . _ % + -
        /// Domain: alphanumeric plus . - (must contain TLD, must end with alphanumeric)
        ///
        /// - Domain must contain at least one dot (e.g., `foo@bar` is not valid,
        ///   `foo@bar.com` is)
        ///
        /// - Domain must end with alphanumeric (prevents capturing trailing punctuation
        ///   like `user@example.com.` - the dot stays outside the email for sentence
        ///   endings)
        rule email_address() -> &'input str
        = email:$(
            // Local part: quoted ("Jane Doe"@example.com) or unquoted
            ("\"" [^'"']+ "\"" / ['a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '%' | '+' | '-']+)
            "@"
            // Domain: alphanumeric+ (separator alphanumeric+)*
            // Ensures domain ends with alphanumeric (not . or -) and has proper structure.
            ['a'..='z' | 'A'..='Z' | '0'..='9']+
            (['.' | '-'] ['a'..='z' | 'A'..='Z' | '0'..='9']+)*
        )
        {?
            // Require TLD - domain must contain at least one dot after @
            let domain = email.rsplit_once('@').map(|(_, d)| d).ok_or("invalid email")?;
            if !domain.contains('.') {
                return Err("email domain must have TLD (contain a dot)");
            }
            Ok(email)
        }

        /// URL path component - supports query params, fragments, encoding, etc.
        /// Excludes '[' and ']' to respect AsciiDoc macro/attribute boundaries
        rule url_path() -> String = path:$(['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%' | '\\' ]+)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                &state.input,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess url path");
                "could not preprocess url path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_warning(warning);
            }
            // Strip backslash escapes before URL parsing to prevent the url crate
            // from normalizing backslashes to forward slashes
            Ok(strip_url_backslash_escapes(&processed.text))
        }

        /// URL for bare autolinks — avoids capturing trailing sentence punctuation
        /// (., ;, !, etc.) by only consuming punctuation when more URL chars follow.
        rule bare_url() -> String =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:bare_url_path()
        { format!("{proto}://{path}") }

        /// URL path for bare autolinks. Like url_path() but:
        /// - Trailing punctuation (. , ; ! ? : ' *) only consumed when followed by more URL chars.
        /// - `)` only consumed as part of a balanced `(...)` group, preventing capture of
        ///   sentence-level parens like `(see http://example.com)`.
        rule bare_url_path() -> String = path:$(
            bare_url_safe_char()
            ( bare_url_safe_char()
            / bare_url_paren_group()
            / "("
            / bare_url_trailing_char() &bare_url_char()
            )*
        )
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                &state.input,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
                .map_err(|e| {
                    tracing::error!(?e, "could not preprocess bare url path");
                    "could not preprocess bare url path"
                })?;
            for warning in inline_state.drain_warnings() {
                state.add_warning(warning);
            }
            Ok(strip_url_backslash_escapes(&processed.text))
        }

        /// Balanced parenthesized group in a URL path.
        /// Handles nested parens: `http://example.com/wiki/Foo_(bar_(baz))`
        /// Only `)` consumed via this rule — unbalanced `)` is never captured.
        rule bare_url_paren_group()
        = "(" (bare_url_safe_char() / bare_url_trailing_char() / bare_url_paren_group() / "(")* ")"

        /// URL chars that are safe to end a bare URL — won't be confused with sentence punctuation.
        /// Excludes `(` and `)` which are handled separately via `bare_url_paren_group`.
        rule bare_url_safe_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '~'
            | '/' | '#' | '@' | '$' | '&'
            | '+' | '=' | '%' | '\\']

        /// URL chars that are valid mid-URL but should not end a bare URL.
        /// Excludes `)` which is only consumed via balanced `bare_url_paren_group`.
        rule bare_url_trailing_char() = ['.' | ',' | ';' | '!' | '?' | ':' | '\'' | '*']

        /// Any valid URL path char (for lookahead in trailing char rule).
        /// Includes `(` because it can start a paren group.
        /// Excludes `)` so that trailing chars before `)` aren't greedily consumed
        /// (e.g., `http://example.com.)` keeps both `.` and `)` outside).
        rule bare_url_char() = bare_url_safe_char() / bare_url_trailing_char() / "("

        /// Fragment identifier for URLs and cross-references (e.g., `#section-id`)
        /// Only used by `xref:` and `link:` macros — other macros (`image::`, `video::`, etc.) do not support fragments
        rule path_fragment() -> &'input str
            = $("#" ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+)

        /// Filesystem path - conservative character set for cross-platform compatibility
        /// Includes '{' and '}' for `AsciiDoc` attribute substitution
        pub rule path() -> String = path:$(['A'..='Z' | 'a'..='z' | '0'..='9' | '{' | '}' | '_' | '-' | '.' | '/' | '\\' ]+)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                &state.input,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess path");
                "could not preprocess path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_warning(warning);
            }
            Ok(processed.text)
        }

        pub rule source() -> Source
            = source:
        (
            u:url() {?
                Source::from_str(&u).map_err(|_| "failed to parse URL")
            }
            / p:path() {?
                Source::from_str(&p).map_err(|_| "failed to parse path")
            }
        )
        { source }

        rule digits() = ['0'..='9']+

        rule whitespace() = quiet!{ " " / "\t" }
        rule eol() = quiet!{ "\n" }

        rule position() -> PositionWithOffset = offset:position!() {
            PositionWithOffset {
                offset,
                position: state.line_map.offset_to_position(offset, &state.input)
            }
        }
    }
}
