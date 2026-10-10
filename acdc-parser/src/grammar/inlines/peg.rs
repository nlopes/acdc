use crate::{
    Anchor, AttributeValue, Autolink, BlockMetadata, Bold, Button, CurvedApostrophe,
    CurvedQuotation, Footnote, Form, Highlight, ICON_SIZES, Icon, Image, InlineMacro, InlineNode,
    Italic, Keyboard, LineBreak, Link, Mailto, Menu, Monospace, Pass, PassthroughKind, Plain, Raw,
    Source, StandaloneCurvedApostrophe, Stem, StemNotation, Subscript, Superscript, Title, Url,
    grammar::{
        ParserState,
        helpers::{
            BlockParsingMetadata, MacroAttributeContext, PositionWithOffset,
            RESERVED_NAMED_ATTRIBUTE_ID, RESERVED_NAMED_ATTRIBUTE_OPTIONS,
            RESERVED_NAMED_ATTRIBUTE_ROLE, is_valid_bibliography_id, restore_url_path,
        },
        inline_boundaries::{check_constrained_closing_at_end, check_constrained_opening_boundary},
        inline_preprocessing,
        inline_preprocessor::InlinePreprocessorParserState,
        inline_processing::{process_inlines, process_inlines_no_autolinks},
        inlines::{
            attributes::{Shorthand, process_attribute_list},
            index_terms::{
                IndexTermForm, IndexTermMacroItem, IndexTermRelationshipSegments, IndexTermSegment,
                expand_escaped_index_terms, parse_index_term, trimmed_index_term_segment,
            },
            links::{LinkContent, ProcessedLinkContent, process_link_content, xref_macro_text},
            recognition::{
                byte_came_from_attribute, catalog_escape_allowed, catalog_macro_allowed,
                check_code_opening_boundary, code_followed_by_attribute, has_at_sign_ahead,
                has_inline_line_break_prefix, index_content_present, is_plain_text_safe,
                macro_token_allowed, record_url_boundary, structural_token_allowed,
                url_opening_allowed,
            },
            text::{
                parse_footnote_content, process_inlines_or_err, process_unescaped_xref_label,
                registration_source,
            },
        },
        state::{InlineRules, InlineUrlBoundary, ParserScope},
    },
    model::{strip_quotes, substitution::HEADER},
};
use std::ops::Range;

peg::parser! {
    pub(crate) grammar inline_parser(state: &mut ParserState<'input>) for str {
        use std::{borrow::Cow, str::FromStr};
        use crate::model::{substitute, Substitution, substitution::parse_substitution};

        // Each action gets the byte range of its preceding sequence.
        inject span_start(_input, l, _r) -> usize { l }
        inject span_end(_input, _l, r) -> usize { r }

        // Group consecutive ordinary nodes so profiles need no vector per node.
        // Expansions still run at their source position, before later macros.
        pub(crate) rule inlines() -> Vec<InlineNode<'input>>
        = check_attribute_profiles() chunks:(profiled_attribute() / nodes:normal_inline()+ { nodes })+ {? expand_escaped_index_terms(chunks.into_iter().flatten().collect(), state) }
        / nodes:normal_inline()+ {? expand_escaped_index_terms(nodes, state) }

        rule normal_inline() -> InlineNode<'input>
        = node:(non_plain_text() / plain_text()) {
            record_url_boundary(state, &node, span_end, &Substitution::Quotes);
            node
        }

        pub(crate) rule inlines_no_autolinks() -> Vec<InlineNode<'input>>
        = inlines()

        pub(crate) rule verbatim_inlines() -> Vec<InlineNode<'input>>
        = check_attribute_profiles() chunks:(profiled_attribute() / nodes:verbatim_inline()+ { nodes })+ {? expand_escaped_index_terms(chunks.into_iter().flatten().collect(), state) }
        / nodes:verbatim_inline()+ {? expand_escaped_index_terms(nodes, state) }

        rule verbatim_inline() -> InlineNode<'input>
        = node:(verbatim_index_term() / verbatim_footnote() / verbatim_anchor() / verbatim_link() / quotes_non_plain_text() / verbatim_plain_text()) {
            record_url_boundary(state, &node, span_end, &Substitution::Quotes);
            node
        }

        rule check_attribute_profiles()
        = {? (!state.attribute_passthroughs.is_empty()).then_some(()).ok_or("no attribute profiles") }

        rule profiled_attribute_match() -> usize
        = "���" index:$(digits()) "���" {?
            index.parse::<usize>().ok().filter(|index| {
                state.attribute_passthroughs.get(*index).is_some_and(|pass| !pass.attribute_fragments.is_empty())
            }).ok_or("not a profiled attribute")
        }

        rule profiled_attribute() -> Vec<InlineNode<'input>>
        = index:profiled_attribute_match() {
            let nodes = crate::grammar::passthrough_processing::process_attribute_placeholder(index, span_start, span_end, state);
            // Empty profiles keep the preceding boundary; nonempty profiles
            // use their final nonempty node rather than the opaque placeholder.
            if let Some(node) = nodes.iter().rfind(|node|
                !matches!(node, InlineNode::PlainText(text) if text.content.is_empty())
                    && !matches!(node, InlineNode::RawText(text) if text.content.is_empty())) {
                record_url_boundary(state, node, span_end, &Substitution::Attributes);
            } else {
                state.last_url_boundary = match state.last_url_boundary {
                    Some(InlineUrlBoundary::Formatted(end)) if end == span_start =>
                        Some(InlineUrlBoundary::Formatted(span_end)),
                    Some(InlineUrlBoundary::Protected(end)) if end == span_start =>
                        Some(InlineUrlBoundary::Protected(span_end)),
                    Some(InlineUrlBoundary::Text(end, previous)) if end == span_start =>
                        Some(InlineUrlBoundary::Text(span_end, previous)),
                    _ => Some(InlineUrlBoundary::Text(span_end, state.input[..span_start].chars().next_back())),
                };
            }
            nodes
        }

        rule verbatim_anchor() -> InlineNode<'input>
        = check_macros() node:(
            node:escaped_anchor_macro() { node }
            / &("\\"+ "[[") node:escaped_syntax() { node }
            / node:inline_anchor() { node }
        ) { node }

        rule verbatim_footnote() -> InlineNode<'input>
        = check_macros() node:(
            "\\" &footnote_match() content:$("footnote:" id()? "[") {
                InlineNode::PlainText(Plain {
                    content,
                    location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                    escaped: false,
                })
            }
            / node:footnote() { node }
        ) { node }

        rule verbatim_link() -> InlineNode<'input>
        = check_macros() node:(
            &("\\"+ verbatim_link_match(true)) node:escaped_syntax() { node }
            / "\\" content:$(check_autolinks() inline_autolink_match(true)) {
                InlineNode::PlainText(Plain {
                    content,
                    location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                    escaped: false,
                })
            }
            / &['<'] node:cross_reference_shorthand() { node }
            / &['x'] node:cross_reference_macro() { node }
            / &['l'] node:link_macro() { node }
            / &['l'] node:literal_link_target() { node }
            / &['m'] node:mailto_macro() { node }
            / &['h' | 'f'] node:url_macro() { node }
            / check_autolinks() node:inline_autolink() { node }
        ) { node }

        rule verbatim_link_match(escaped: bool)
        = cross_reference_shorthand_match() / cross_reference_macro_match()
        / link_macro_match() / mailto_macro_match() / url_macro_match(escaped)
        / literal_link_target_match()
        / check_autolinks() inline_autolink_match(escaped)

        rule verbatim_index_term() -> InlineNode<'input>
        = check_index_terms() node:(
            &['\\'] node:escaped_index_prefix() { node }
            / &("\\"+ index_term_match()) node:escaped_syntax() { node }
            / &['('] node:index_term_concealed() { node }
            / &['('] node:index_term_flow() { node }
            / &['i'] node:indexterm_macro() { node }
            / &['i'] node:indexterm2_macro() { node }
        ) { node }

        rule verbatim_plain_text() -> InlineNode<'input>
        = content:$((
            !profiled_attribute_match()
            !(check_index_terms() (
                &("\\"+ index_term_match()) escaped_syntax_match() / index_term_match()
            ))
            !(check_macros() ("\\"? footnote_match()))
            !(check_macros() (
                "\\"? anchor_macro_pattern() / inline_anchor_match()
                / &("\\"+ "[[") escaped_syntax_match()
            ))
            !(check_macros() (
                verbatim_link_match(false)
                / &("\\"+ verbatim_link_match(true)) escaped_syntax_match()
                / "\\" check_autolinks() inline_autolink_match(true)
            ))
            !(check_quotes() (
                escaped_syntax_match() / bold_text_unconstrained_match() / bold_text_constrained_match()
                / italic_text_unconstrained_match() / italic_text_constrained_match()
                / monospace_text_unconstrained_match() / monospace_text_constrained_match()
                / highlight_text_unconstrained_match() / highlight_text_constrained_match()
                / superscript_text_match() / subscript_text_match()
                / curved_quotation_text_match() / curved_apostrophe_text_match() / standalone_curved_apostrophe_match()
            ))
            [_]
        )+) {
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        /// Reduced inline rule set for "quotes" substitution in passthroughs.
        /// Only matches formatting markup + escaped markup + plain text.
        /// Explicit attribute profiles can insert their own enabled constructs.
        pub(crate) rule quotes_only_inlines() -> Vec<InlineNode<'input>>
        = check_attribute_profiles() chunks:(profiled_attribute() / nodes:quotes_inline()+ { nodes })+ { chunks.into_iter().flatten().collect() }
        / quotes_inline()+

        rule quotes_inline() -> InlineNode<'input>
        = quotes_non_plain_text() / quotes_plain_text()

        /// Non-plain-text alternatives for quotes-only mode: formatting markup only.
        /// Keep in sync with the formatting entries in `non_plain_text` above.
        rule quotes_non_plain_text() -> InlineNode<'input>
        = check_quotes() inline:(
            escaped_super_sub:escaped_superscript_subscript() { escaped_super_sub }
            / escaped_syntax:escaped_syntax() { escaped_syntax }
            / bold_text_unconstrained:bold_text_unconstrained() { bold_text_unconstrained }
            / bold_text_constrained:bold_text_constrained() { bold_text_constrained }
            / italic_text_unconstrained:italic_text_unconstrained() { italic_text_unconstrained }
            / italic_text_constrained:italic_text_constrained() { italic_text_constrained }
            / monospace_text_unconstrained:monospace_text_unconstrained() { monospace_text_unconstrained }
            / monospace_text_constrained:monospace_text_constrained() { monospace_text_constrained }
            / highlight_text_unconstrained:highlight_text_unconstrained() { highlight_text_unconstrained }
            / highlight_text_constrained:highlight_text_constrained() { highlight_text_constrained }
            / superscript_text:superscript_text() { superscript_text }
            / subscript_text:subscript_text() { subscript_text }
            / curved_quotation_text:curved_quotation_text() { curved_quotation_text }
            / curved_apostrophe_text:curved_apostrophe_text() { curved_apostrophe_text }
            / standalone_curved_apostrophe:standalone_curved_apostrophe() { standalone_curved_apostrophe }
        ) {
            inline
        }

        /// Plain text for quotes-only mode: reduced negative lookaheads (formatting patterns only).
        /// Keep in sync with the formatting lookaheads in `plain_text` below.
        rule quotes_plain_text() -> InlineNode<'input>
        = start_pos:position!()
        content:$((
            "\\" "^" !([^'^' | ' ' | '\t' | '\n']+ "^")
            / "\\" "~" !([^'~' | ' ' | '\t' | '\n']+ "~")
            // Fast path: characters that can never start any quotes inline construct.
            // Fewer triggers than plain_text since quotes context has no macros/autolinks.
            / [^('\n' | '\r' | '\\' | '[' | '*' | '_' | '`' | '#' | '^' | '~' | '"' | '\'' | '\u{FFFD}')]+
            / (
                !(
                    profiled_attribute_match()
                    / paragraph_break()
                    / ![_]
                    / &['\\'] escaped_syntax_match()
                    / check_quotes() &['*' | '_' | '`' | '#' | '^' | '~' | '"' | '\'' | '['] (
                        bold_text_unconstrained_match() / bold_text_constrained_match() / italic_text_unconstrained_match() / italic_text_constrained_match() / monospace_text_unconstrained_match() / monospace_text_constrained_match() / highlight_text_unconstrained_match() / highlight_text_constrained_match() / superscript_text_match() / subscript_text_match() / curved_quotation_text_match() / curved_apostrophe_text_match() / standalone_curved_apostrophe_match()
                    )
                )
                [_]
            )
        )+)
        end:position!()
        {
            tracing::debug!(input_len = content.len(), "Found quotes-only plain text inline");
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(start_pos, end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        rule non_plain_text() -> InlineNode<'input>
        = inline:(
            // Each alternative is prefixed with a `&[...]` byte-lookahead that
            // rejects most call sites before the real rule is entered. On
            // macro-dense docs `non_plain_text` was the top hotspot: packrat
            // memoisation pays its fair share, but the alternation still had
            // to TRY each of ~30 rules on every candidate position. The
            // byte guards cut that to the subset whose first byte matches.
            // Preserves original ordering to keep PEG first-match semantics.

            // Escaped superscript/subscript must come first - produces RawText to prevent re-parsing
            &['\\'] escaped_super_sub:escaped_superscript_subscript() { escaped_super_sub }
            / &['\\'] check_index_terms() prefix:escaped_index_prefix() { prefix }
            // Escaped syntax must come next - backslash prevents any following syntax from being parsed
            / check_macros() &['\\'] anchor:escaped_anchor_macro() { anchor }
            / &['\\'] bracket:index_label_bracket() { bracket }
            / &['\\'] bracket:escaped_label_character() { bracket }
            / &['\\'] escaped_syntax:escaped_syntax() { escaped_syntax }
            // Index terms: concealed (triple parens) must come before flow (double parens)
            / check_index_terms() &['('] index_term:index_term_concealed() { index_term }
            / check_index_terms() &['('] index_term:index_term_flow() { index_term }
            / check_index_terms() &['i'] indexterm:indexterm_macro() { indexterm }
            / check_index_terms() &['i'] indexterm2:indexterm2_macro() { indexterm2 }
            / check_macros() &['['] invalid_bibliography_anchor:invalid_bibliography_anchor() { invalid_bibliography_anchor }
            / check_macros() &['[' | 'a'] inline_anchor:inline_anchor() { inline_anchor }
            / check_macros() &['<'] cross_reference_shorthand:cross_reference_shorthand() { cross_reference_shorthand }
            / check_macros() &['x'] cross_reference_macro:cross_reference_macro() { cross_reference_macro }
            / check_hardbreaks() &['\n' | '\r'] automatic_line_break:automatic_line_break() { automatic_line_break }
            / check_post_replacements() &[' '] hard_wrap:hard_wrap() { hard_wrap }
            / check_macros() &"footnote:" footnote:footnote() { footnote }
            / check_macros() &['s' | 'a' | 'l'] stem:inline_stem() { stem }
            / check_macros() &['i'] image:inline_image() { image }
            / check_macros() &['i'] icon:inline_icon() { icon }
            / check_macros() &['k'] keyboard:inline_keyboard() { keyboard }
            / check_macros() &['b'] button:inline_button() { button }
            / check_macros() &['m'] menu:inline_menu() { menu }
            // mailto has to come before the url_macro because url_macro calls url() which
            // also matches against mailto:
            / check_macros() &['m'] mailto_macro:mailto_macro() { mailto_macro }
            / check_macros() &['h' | 'f'] url_macro:url_macro() { url_macro }
            / check_macros() &['p'] pass:inline_pass() { pass }
            / check_macros() &['l'] link_macro:link_macro() { link_macro }
            / check_macros() &['l'] target:literal_link_target() { target }
            // No byte guard: autolink matches bare URLs (`h`/`f` schemes) AND bare
            // emails (any ASCII alphanumeric), and `<url>` / `<email>`. The
            // upstream `plain_text_quick_safe` fast path already rejects most
            // email candidates before we reach non_plain_text, so this
            // alternative is only tried on positions that actually need it.
            / check_macros() check_autolinks() inline_autolink:inline_autolink() { inline_autolink }
            / check_post_replacements() &[' '] inline_line_break:inline_line_break() { inline_line_break }
            / check_quotes() &['[' | '*'] bold_text_unconstrained:bold_text_unconstrained() { bold_text_unconstrained }
            / check_quotes() &['[' | '*'] bold_text_constrained:bold_text_constrained() { bold_text_constrained }
            / check_quotes() &['[' | '_'] italic_text_unconstrained:italic_text_unconstrained() { italic_text_unconstrained }
            / check_quotes() &['[' | '_'] italic_text_constrained:italic_text_constrained() { italic_text_constrained }
            / check_quotes() &['[' | '`'] monospace_text_unconstrained:monospace_text_unconstrained() { monospace_text_unconstrained }
            / check_quotes() &['[' | '`'] monospace_text_constrained:monospace_text_constrained() { monospace_text_constrained }
            / check_quotes() &['[' | '#'] highlight_text_unconstrained:highlight_text_unconstrained() { highlight_text_unconstrained }
            / check_quotes() &['[' | '#'] highlight_text_constrained:highlight_text_constrained() { highlight_text_constrained }
            / check_quotes() &['[' | '^'] superscript_text:superscript_text() { superscript_text }
            / check_quotes() &['[' | '~'] subscript_text:subscript_text() { subscript_text }
            / check_quotes() &['[' | '"'] curved_quotation_text:curved_quotation_text() { curved_quotation_text }
            / check_quotes() &['[' | '\''] curved_apostrophe_text:curved_apostrophe_text() { curved_apostrophe_text }
            / check_quotes() &['`'] standalone_curved_apostrophe:standalone_curved_apostrophe() { standalone_curved_apostrophe }
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
        rule escaped_superscript_subscript() -> InlineNode<'input>
        = check_catalog_escape() check_quotes() "\\" content:escaped_super_sub_pattern() {
            InlineNode::PlainText(Plain {
                content,
                location: state.create_location(span_start + state.inline_ctx.offset, span_end + state.inline_ctx.offset),
                escaped: true,
            })
        }

        /// Match escaped superscript (^content^) or subscript (~content~) pattern.
        rule escaped_super_sub_pattern() -> &'input str
        = "^" inner:$([^'^' | ' ' | '\t' | '\n']+) "^" { state.intern_fmt(format_args!("^{inner}^")) }
        / "~" inner:$([^'~' | ' ' | '\t' | '\n']+) "~" { state.intern_fmt(format_args!("~{inner}~")) }

        // Active bracket-delimited labels consume one closing-bracket escape.
        // Quote escapes apply only inside quoted link labels; the remaining text
        // still needs backend escaping, but no further AsciiDoc replacements.
        rule escaped_label_character() -> InlineNode<'input>
        = content:$(escaped_label_character_match()) {
            let escapes = usize::from(structural_token_allowed(
                state, &Substitution::Macros, span_start, content.len(),
            ));
            InlineNode::RawText(Raw {
                content: &content[escapes..],
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                subs: vec![Substitution::SpecialChars],
            })
        }

        rule escaped_label_character_match()
        = check_bracket_label() "\\"+ (
            "]"
            / check_link_quote(InlineRules::DOUBLE_QUOTED_LINK) "\""
            / check_link_quote(InlineRules::SINGLE_QUOTED_LINK) "'"
        )

        // A shorthand index has no square-bracket delimiter. Named macros and
        // enclosing labels each consume one closing-bracket escape, in that order.
        // Raw text protects remaining backslashes from converter replacements.
        rule index_label_bracket() -> InlineNode<'input>
        = content:$(index_label_bracket_match()) {
            let rules = state.inline_ctx.rules;
            // A later attribute value did not exist when either macro consumed escapes.
            let escapes = if content.ends_with(']')
                && structural_token_allowed(state, &Substitution::Macros, span_start, content.len())
            {
                usize::from(rules.contains(InlineRules::NAMED_INDEX_LABEL))
                    + usize::from(rules.contains(InlineRules::BRACKET_LABEL))
            } else {
                0
            };
            InlineNode::RawText(Raw {
                content: &content[escapes.min(content.len() - 1)..],
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                subs: vec![Substitution::SpecialChars],
            })
        }

        // Complete escaped anchors still belong to the ordinary macro escape rule.
        rule index_label_bracket_match()
        = "\\"+ !("[[" (!"]]" [_])* "]]") ['[' | ']']
          {? state.inline_ctx.rules.contains(InlineRules::INDEX_LABEL).then_some(()).ok_or("index label") }

        rule check_link_quote(mode: InlineRules)
        = {? state.inline_ctx.rules.contains(mode).then_some(()).ok_or("quoted link label") }

        rule check_bracket_label()
        = {? state.inline_ctx.rules.contains(InlineRules::BRACKET_LABEL).then_some(()).ok_or("bracket-delimited label") }

        /// Remove the escape before recognized inline syntax, keeping its text literal.
        rule escaped_syntax() -> InlineNode<'input>
        = check_catalog_escape() escapes:$("\\"+) check_macros() content:$(url_macro_match(true) / check_autolinks() bare_url(true)) {
            // Asciidoctor recognizes a bare URI escape only with one backslash.
            // A longer run leaves the entire URL macro literal, including the run.
            // Late attribute text also stays literal: the original reference's
            // closing brace prevents URI recognition before attributes expand.
            InlineNode::PlainText(Plain {
                content: if escapes.len() == 1 && macro_token_allowed(state, span_start, 1) {
                    content
                } else {
                    &state.input[span_start..span_end]
                },
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }
        / check_catalog_escape() "\\" content:escaped_content() {
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        // Typography escapes stay intact for the converter's replacement stage.
        rule escaped_content() -> &'input str
        = escapable_macro_pattern()
        // Double backslash followed by escapable pattern: \\<thing> -> <thing>
        / "\\" inner:escapable_pattern() { inner }
        // Single backslash case: \<thing> -> <thing>
        / escapable_pattern()

        // Recognition must use the enabled macro's complete syntax. Otherwise
        // disabled macros, unknown names, and escaped closing brackets lose text.
        rule escapable_macro_pattern() -> &'input str
        = check_macro_escape() content:$(
            check_index_terms() index_term_match()
            / check_macros()
            // The URI pass consumes link: before named macro escapes are handled.
            !("link:" url_macro_syntax())
            (cross_reference_shorthand_match() / cross_reference_macro_match()
            / footnote_match() / link_macro_match() / mailto_macro_match()
            // These empty forms still recognize an escape in Asciidoctor.
            / "footnote:[]" / "link:[" ("\\]" / !"]" [_])* "]"
            / inline_image_match() / inline_icon_match() / inline_stem_match()
            / inline_keyboard_match() / inline_button_match() / inline_menu_match()
            / inline_pass_match() / inline_anchor_match())
        ) { content }

        /// Non-macro patterns that can be escaped with a backslash.
        rule escapable_pattern() -> &'input str
        = check_quotes() content:(
        // Formatting escapes belong to quote substitution. Code and labels
        // with quotes disabled must retain the complete authored text.
        // Attribute escapes are consumed by the attribute stage, not formatting.
        // Unconstrained formatting: match entire span including content and closing marker
        // \**not bold** -> **not bold**
        "**" inner:$((!"**" [_])*) "**" { state.intern_fmt(format_args!("**{inner}**")) }
        / "__" inner:$((!("__" !['_']) [_])*) "__" { state.intern_fmt(format_args!("__{inner}__")) }
        / "``" inner:$((!"``" [_])*) "``" { state.intern_fmt(format_args!("``{inner}``")) }
        / "##" inner:$((!"##" [_])*) "##" { state.intern_fmt(format_args!("##{inner}##")) }
        // Typography patterns are NOT handled here — they are handled by the
        // converter's strip_backslash_escapes() pipeline. If the parser stripped the
        // backslash, the converter would never see it and would apply the replacement.
        //
        // Superscript: ^content^ where content has no whitespace (must check complete pattern)
        / "^" inner:$([^'^' | ' ' | '\t' | '\n']+) "^" { state.intern_fmt(format_args!("^{inner}^")) }
        // Subscript: ~content~ where content has no whitespace (must check complete pattern)
        / "~" inner:$([^'~' | ' ' | '\t' | '\n']+) "~" { state.intern_fmt(format_args!("~{inner}~")) }
        // Bracket escapes belong to active macro labels, not ordinary text.
        // Constrained formatting markers and other single escapable chars
        // Note: ^ and ~ are NOT included here - they require complete patterns above
        / c:$(['*' | '_' | '#' | '`' | '&']) { c }
        ) { content }

        /// Match escaped syntax without consuming - for use in negative lookaheads.
        rule escaped_syntax_match() -> ()
        = check_catalog_escape() (
            "\\"+ check_macros() (url_macro_match(true) / check_autolinks() bare_url(true))
            / "\\" (escapable_macro_pattern() / "\\"? escapable_pattern_match())
        )

        /// Match escapable patterns without consuming
        rule escapable_pattern_match() -> ()
        = check_quotes() (
        // Unconstrained formatting: match entire span
        "**" (!"**" [_])* "**"
        / "__" (!("__" !['_']) [_])* "__"
        / "``" (!"``" [_])* "``"
        / "##" (!"##" [_])* "##"
        // Typography patterns handled by converter, not here (see escapable_pattern)
        // Superscript/subscript: require complete pattern
        / "^" [^'^' | ' ' | '\t' | '\n']+ "^"
        / "~" [^'~' | ' ' | '\t' | '\n']+ "~"
        // Single escapable chars (excluding ^ and ~ which need complete patterns)
        / ['*' | '_' | '#' | '`' | '&'] {}
        )

        rule footnote() -> InlineNode<'input>
        = footnote_match:footnote_match()
        {?
            let (start, id, content_start, content_str, end) = footnote_match;

            tracing::debug!(input_len = content_str.len(), start, end, "Found footnote inline");

            // Repeated definitions use the first body without registering its macros again.
            let content = if content_str.is_empty()
                || id.is_some_and(|id| state.footnote_tracker.borrow().contains(id))
            {
                vec![]
            } else if content_str.trim().is_empty() {
                vec![InlineNode::PlainText(Plain {
                    content: content_str,
                    location: state.create_block_location(content_start, end - 1, state.inline_ctx.offset),
                    escaped: false,
                })]
            } else {
                // The note body belongs to the footnote, not the link's quoted attribute.
                let rules = state.inline_ctx.rules;
                state.inline_ctx.rules.remove(InlineRules::QUOTED_LINK);
                state.inline_ctx.rules.insert(InlineRules::BRACKET_LABEL);
                let content = parse_footnote_content(state, IndexTermSegment { text: content_str, start: content_start });
                state.inline_ctx.rules = rules;
                content?
            };

            let plan = state.inline_ctx.substitutions;
            let registration_substitutions = [Substitution::Attributes, Substitution::Quotes, Substitution::Replacements]
                .iter().any(|stage| plan.precedes(&Substitution::Macros, stage)).then_some(plan);
            let mut footnote = Footnote {
                definition_source: (id.is_some() && !content_str.is_empty()).then(|| {
                    registration_source(state, IndexTermSegment { text: content_str, start: content_start }).0
                }),
                registration_substitutions,
                id,
                content,
                number: 0,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            };
            if !state.footnote_tracker.borrow_mut().push(&mut footnote) {
                let id = id.unwrap_or_default();
                state.add_generic_warning(format!("invalid footnote reference: {id}"));
                return Ok(InlineNode::PlainText(Plain {
                    content: state.intern_fmt(format_args!("[{id}]")),
                    location: footnote.location,
                    escaped: false,
                }));
            }

            Ok(InlineNode::Macro(InlineMacro::Footnote(footnote)))
        }

        rule footnote_match() -> (usize, Option<&'input str>, usize, &'input str, usize)
        = "footnote:"
        id:id()? "[" content_start:position!() content:footnote_content() "]"
        {?
            (id.is_some() || !content.is_empty())
                .then_some((span_start, id, content_start, content, span_end))
                .ok_or("footnote body or reference id")
        }

        // A closing-bracket escape belongs to the body. Preserve its source bytes
        // until label parsing removes it, including for registration-time text.
        rule footnote_content() -> &'input str
        = content:$(footnote_content_part()*) { content }

        rule footnote_content_part()
        = "\\" ['[' | ']']
        / "[" footnote_content() "]"
        / !("\\" ['[' | ']'] / "[" / "]") [_]

        /// Parse content that may contain balanced square brackets (general case)
        /// This is used for nested link brackets and button labels
        rule balanced_bracket_content() -> &'input str
        = content:$(balanced_bracket_content_part()*) { content }

        /// Individual parts of balanced bracket content - either regular text or nested brackets
        rule balanced_bracket_content_part() -> Cow<'input, str>
        = nested_brackets:("[" inner:balanced_bracket_content() "]" { Cow::Owned(format!("[{inner}]")) })
        / regular_text:$([^('[' | ']')]+) { Cow::Borrowed(regular_text) }

        /// Parse content within brackets using escape handling (no bracket balancing).
        /// Used for stem macros where `\]` and `\[` are common (math notation).
        /// - `\\` → literal `\`
        /// - `\[` or `\]` → literal bracket (backslash stripped)
        /// - `\` before other chars → preserved
        /// - `[` → regular text (no nesting)
        /// - `]` → ends content (consumed by caller)
        rule escaped_bracket_content() -> &'input str
        = parts:escaped_bracket_content_part()* {
            match parts.as_slice() {
                [one] => *one,
                _ => state.intern_join(parts.iter(), ""),
            }
        }

        rule escaped_bracket_content_part() -> &'input str
        = "\\\\" { "\\" }
        / "\\" c:$(['[' | ']']) { c }
        / s:$([^(']' | '\\')]+) { s }
        / "\\" { "\\" }

        // Nested macro delimiters belong to their child nodes, not the outer
        // attribute list. Keep their spans relative to this bracket content.
        rule link_macro_content() -> LinkContent<'input>
        = start:position!() parts:link_macro_content_part()* end:position!() {
            LinkContent {
                raw: &state.input[start..end],
                protected: parts.into_iter().flatten().map(|range| range.start - start..range.end - start).collect(),
            }
        }

        rule link_macro_content_part() -> Option<Range<usize>>
        = "\\]" { None }
        / "[" balanced_bracket_content() "]" { Some(span_start..span_end) }
        // '(' starts shorthand; 'i' starts indexterm:/indexterm2:. Both forms
        // own their brackets, which must not terminate the enclosing link.
        / &['(' | 'i'] check_index_terms() index_term_match() { Some(span_start..span_end) }
        / &['<'] cross_reference_shorthand_match() { Some(span_start..span_end) }
        / (!("\\]" / (&['i'] check_index_terms() index_term_match())) [^'[' | ']' | '(' | '<'])+ { None }
        / ['[' | '(' | '<'] { None }

        rule inline_pass() -> InlineNode<'input>
        = check_pass_token() "pass:"
        substitutions:($([^('[' | ']' | ',')]+) ** comma())
        "["
        content:$(("\\]" / [^']'])*)
        "]"
        {
            tracing::debug!("Found pass inline");
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            InlineNode::Macro(InlineMacro::Pass(Pass {
                attribute_fragments: Box::default(),
                text: Some(content),
                substitutions: substitutions.into_iter().filter_map(|s| parse_substitution(s.trim())).collect(),
                location,
                kind: PassthroughKind::Macro,
            }))
        }

        /// Match inline pass without consuming - for use in negative lookaheads.
        rule inline_pass_match()
        = check_pass_token() "pass:" ([^('[' | ']' | ',')]+ ("," [^('[' | ']' | ',')]+)*)? "[" ("\\]" / [^']'])* "]"

        // Whole-value pass wrappers are resolved at definition time. Substitution must
        // not activate a wrapper that remains inside the stored attribute value.
        rule check_pass_token()
        = start:position!() {?
            if let Some((names, _)) = state.input[start..].strip_prefix("pass:").and_then(|tail| tail.split_once('['))
                && (start..start + 6 + names.len()).any(|pos| byte_came_from_attribute(state, pos))
            {
                Err("passthrough introduced by attribute")
            } else {
                Ok(())
            }
        }

        rule index_term_concealed() -> InlineNode<'input>
        = content:index_term_concealed_content() {?
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            parse_index_term(state, content, IndexTermForm::Concealed, location)
        }

        rule index_term_flow() -> InlineNode<'input>
        = content:index_term_flow_content() {?
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            parse_index_term(state, content, IndexTermForm::Flow, location)
        }

        rule indexterm_macro() -> InlineNode<'input>
        = start:position!() "indexterm:" content:index_term_macro_content() check_index_token(start, 11) {?
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            parse_index_term(state, content, IndexTermForm::NamedConcealed, location)
        }

        rule indexterm2_macro() -> InlineNode<'input>
        = start:position!() "indexterm2:" content:index_term_macro_content() check_index_token(start, 12) {?
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            parse_index_term(state, content, IndexTermForm::NamedFlow, location)
        }

        pub(crate) rule index_term_macro_list(base: usize) -> Vec<IndexTermMacroItem<'input>>
        = items:(index_term_macro_item(base) ** ",") { items }

        rule index_term_macro_item(base: usize) -> IndexTermMacroItem<'input>
        = whitespace()* "see-also" "=" targets:index_term_see_also_value(base) whitespace()* {
            IndexTermMacroItem::Relationship(IndexTermRelationshipSegments::SeeAlso(targets))
        }
        / whitespace()* "see" "=" target:index_term_relation_value(base) whitespace()* {
            IndexTermMacroItem::Relationship(IndexTermRelationshipSegments::See(target))
        }
        / term:index_term_segment(base) { IndexTermMacroItem::Term(term) }

        rule index_term_relation_value(base: usize) -> IndexTermSegment<'input>
        = "\"" start:position!() content:$([^'"']*) "\"" &(whitespace()* ("," / ![_])) {
            trimmed_index_term_segment(content, base + start)
        }
        / start:position!() content:$([^',']*) {
            trimmed_index_term_segment(content, base + start)
        }

        rule index_term_see_also_value(base: usize) -> Vec<IndexTermSegment<'input>>
        = "\"" targets:(index_term_see_also_target(base) ** ",") "\"" &(whitespace()* ("," / ![_])) { targets }
        / target:index_term_relation_value(base) { vec![target] }

        rule index_term_see_also_target(base: usize) -> IndexTermSegment<'input>
        = whitespace()* start:position!() content:$([^(',' | '"')]*) whitespace()* {
            trimmed_index_term_segment(content, base + start)
        }

        /// Parse comma-separated index term list with support for quoted segments
        /// e.g., "knight, Knight of the Round Table, Lancelot"
        /// or "knight, \"Arthur, King\"" (quoted segment with embedded comma)
        pub(crate) rule index_term_list(base: usize) -> Vec<IndexTermSegment<'input>>
        = terms:(index_term_segment(base) ** ",") {
            terms.into_iter().filter(|segment| !segment.text.is_empty()).collect()
        }

        /// Parse a single index term segment, either quoted or unquoted
        rule index_term_segment(base: usize) -> IndexTermSegment<'input>
        = whitespace()? segment:(index_term_quoted(base) / index_term_unquoted(base)) whitespace()? { segment }

        /// Quotes group commas; the macro delimiter can leave the quote unclosed.
        rule index_term_quoted(base: usize) -> IndexTermSegment<'input>
        = "\"" start:position!() content:$([^'"']*) "\""? &(whitespace()* ("," / ![_])) {
            trimmed_index_term_segment(content, base + start)
        }

        /// Unquoted segment: term without comma
        rule index_term_unquoted(base: usize) -> IndexTermSegment<'input>
        = start:position!() content:$([^',']+) {
            trimmed_index_term_segment(content, base + start)
        }

        rule check_index_token(start: usize, len: usize)
        = {? macro_token_allowed(state, start, len).then_some(()).ok_or("index syntax introduced after macros") }

        rule index_term_shorthand_close() = start:position!() "))" check_index_token(start, 2) !")"
        rule index_term_concealed_close() = start:position!() ")" index_term_shorthand_close() check_index_token(start, 3)

        // Concealed shorthand retains its legacy delimiter precedence.
        rule index_term_concealed_content() -> IndexTermSegment<'input>
        = !escaped_index_inner() open:position!() "(((" check_index_token(open, 3) start:position!()
          content:$((!(index_term_concealed_close() / index_term_shorthand_close()) [_])*)
          index_term_concealed_close() {
            IndexTermSegment { text: content, start }
        }

        // Fallback for literal, unbalanced parentheses in pre-spec labels.
        rule index_term_flow_close() = start:position!() "))" check_index_token(start, 2) !"))"

        rule index_term_flow_content() -> IndexTermSegment<'input>
        = (escaped_index_inner() / !"(((") open:position!() "((" check_index_token(open, 2) start:position!()
          content:index_term_flow_body() {?
            index_content_present(state, start, content)
                .then_some(IndexTermSegment { text: content, start })
                .ok_or("empty index term")
        }

        // Preserve internal pairs before selecting the outer delimiter. If the
        // label is not balanced, retain the existing literal-parenthesis rules.
        rule index_term_flow_body() -> &'input str
        = content:$((index_term_parenthesis_group() / "\\" [_] / [^'(' | ')' | '\\'])*)
          close:position!() "))" check_index_token(close, 2) { content }
        / content:$((!index_term_flow_close() [_])*) index_term_flow_close() { content }

        rule index_term_parenthesis_group()
        = "(" (index_term_parenthesis_group() / "\\" [_] / [^'(' | ')' | '\\'])* ")"

        // Escaping a concealed shorthand leaves its inner visible term active.
        rule escaped_index_prefix() -> InlineNode<'input>
        = escapes:$("\\"+) &index_term_concealed_content() "(" {
            InlineNode::PlainText(Plain {
                content: state.intern_fmt(format_args!("{}(", &escapes[..escapes.len() - 1])),
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        rule escaped_index_inner() -> ()
        = pos:position!() {?
            state.input[..pos].ends_with("\\(").then_some(()).ok_or("unescaped index term")
        }

        rule index_term_macro_content() -> IndexTermSegment<'input>
        = "[" start:position!() content:index_term_macro_body() {?
            index_content_present(state, start, content)
                .then_some(IndexTermSegment { text: content, start })
                .ok_or("empty index term")
        }

        // Complete child macros own their delimiters. If no outer close remains,
        // preserve the legacy interpretation, including escaped child brackets.
        // Index terms themselves stay disabled inside an index label.
        rule index_term_macro_body() -> &'input str
        = content:$(("\\]" / formatting_link_match() / bracket_label_macro_match()
            / !index_term_macro_close() [_])*) index_term_macro_close() { content }
        / content:$(("\\]" / !index_term_macro_close() [_])*) index_term_macro_close() { content }

        rule index_term_macro_close() = start:position!() "]" check_index_token(start, 1)

        /// Match index term patterns without consuming (for negative lookahead in plain_text)
        rule index_term_match() -> ()
        = index_term_concealed_content() {}
        / index_term_flow_content() {}
        / start:position!() "indexterm:" index_term_macro_content() check_index_token(start, 11) {}
        / start:position!() "indexterm2:" index_term_macro_content() check_index_token(start, 12) {}

        rule inline_menu() -> InlineNode<'input>
        = check_experimental() "menu:"
        target:$([^'[']+)
        "["
        items:((item:$([^(']' | '>')]+) { item.trim() }) ** (">" whitespace()?))
        "]"
        {
            tracing::debug!("Found menu inline");
            InlineNode::Macro(InlineMacro::Menu(Menu {
                target,
                items,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline menu without consuming - for use in negative lookaheads.
        rule inline_menu_match()
        = check_experimental() "menu:" [^'[']+ "[" ([^']' | '>']+ (">" whitespace()? [^']' | '>']+)*)? "]"

        rule inline_button() -> InlineNode<'input>
        = check_experimental() "btn:[" label:$balanced_bracket_content() "]"
        {
            tracing::debug!("Found button inline");
            InlineNode::Macro(InlineMacro::Button(Button {
                label: label.trim(),
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline button without consuming - for use in negative lookaheads.
        rule inline_button_match()
        = check_experimental() "btn:[" balanced_bracket_content() "]"

        rule inline_keyboard() -> InlineNode<'input>
        = check_experimental() "kbd:["
        keys:((key:$([^(']' | '+' | ',')]+) { key.trim() }) ** (("," / "+") whitespace()?))
        "]"
        {
            tracing::debug!("Found keyboard inline");
            InlineNode::Macro(InlineMacro::Keyboard(Keyboard {
                keys,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline keyboard without consuming - for use in negative lookaheads.
        rule inline_keyboard_match()
        = check_experimental() "kbd:[" [^']' | '+' | ',']+ (("," / "+") whitespace()? [^']' | '+' | ',']+)* "]"

        /// Parse URL macros with attribute handling.
        ///
        /// URL macros have the format: `https://example.com[text,attr1=value1,attr2=value2]`
        ///
        /// This is similar to link macros but the URL is directly specified rather than
        /// using the `link:` prefix.
        rule url_macro() -> InlineNode<'input>
        = check_url_opening(false, false) target:url()
        "["
        content_start:position!() content:link_macro_content() "]"
        {?
            tracing::debug!("Found url macro");
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            let ProcessedLinkContent { text, attributes, .. } = process_link_content(state, &bm, content_start, span_end, &content, false)
                .map_err(|_| {
                    tracing::error!("could not process link text");
                    "could not process link text"
                })?;
            let interned_target = state.intern_cow(target);
            let target_source = Source::from_str_borrowed(interned_target).map_err(|_| "failed to parse URL target")?;
            Ok(InlineNode::Macro(InlineMacro::Url(Url {
                text,
                target: target_source,
                attributes,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                hide_uri_scheme: state.document_attributes.contains_key("hide-uri-scheme"),
            })))
        }

        /// Recognize URL macro syntax for escaping and inline lookahead.
        rule url_macro_match(escaped: bool)
        = check_url_opening(false, escaped) url_macro_syntax()

        // Explicit `link:` macros consume their own prefix and bypass URI boundaries.
        rule url_macro_syntax()
        = url_macro_target_match() "[" ("\\]" / !"]" [_])* "]"

        rule check_url_opening(bare: bool, escaped: bool)
        = position:position!() {?
            url_opening_allowed(state, position, bare, escaped)
                .then_some(()).ok_or("URL opening boundary")
        }

        rule url_macro_target_match()
        = ("https" / "http" / "ftp" / "irc") "://" ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%' | '\\']+

        /// Parse `mailto:` macros with attribute handling.
        ///
        /// `mailto:` macros accept `[text,subject,body]`, followed by named attributes.
        ///
        /// This is similar to link macros but the `mailto:` is directly specified rather
        /// than using the `link:` prefix.
        rule mailto_macro() -> InlineNode<'input>
        = target:$("mailto:" email_address() ("?" url_path_char()*)?)
        "["
        content_start:position!() content:link_macro_content() "]"
        {?
            tracing::debug!("Found mailto macro");
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            let ProcessedLinkContent { text, attributes, subject, body } = process_link_content(state, &bm, content_start, span_end, &content, true)
                .map_err(|_| {
                    tracing::error!("could not process link text");
                    "could not process link text"
                })?;
            let target_source = Source::from_str_borrowed(target).map_err(|_| "failed to parse mailto target")?;
            Ok(InlineNode::Macro(InlineMacro::Mailto(Mailto {
                text,
                subject,
                body,
                target: target_source,
                attributes,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            })))
        }

        /// Match mailto macro without consuming - for use in negative lookaheads.
        /// Inlines the url/email_address patterns to avoid action-block processing.
        rule mailto_macro_match()
        = "mailto:" email_address() ("?" url_path_char()*)? "[" ("\\]" / !"]" [_])* "]"

        rule check_autolinks() -> ()
        = {? if state.inline_ctx.rules.contains(InlineRules::AUTOLINKS) { Ok(()) } else { Err("autolinks suppressed") } }

        rule check_macros() -> ()
        = position:position!() {?
            if state.inline_ctx.substitutions.enabled(&Substitution::Macros)
                && structural_token_allowed(state, &Substitution::Macros, position, 1)
                && catalog_macro_allowed(state, position)
            {
                Ok(())
            } else {
                Err("macros disabled")
            }
        }

        rule check_index_terms() -> ()
        = position:position!() {?
            if state.inline_ctx.rules.contains(InlineRules::INDEX_TERMS)
                && state.inline_ctx.substitutions.enabled(&Substitution::Macros)
                && structural_token_allowed(state, &Substitution::Macros, position, 1)
            {
                Ok(())
            } else {
                Err("index terms suppressed")
            }
        }

        rule check_experimental() -> ()
        = {? if state.document_attributes.contains_key("experimental") { Ok(()) } else { Err("experimental UI macros disabled") } }

        rule check_post_replacements() -> ()
        = {? if state.inline_ctx.substitutions.enabled(&Substitution::PostReplacements) { Ok(()) } else { Err("post_replacements disabled") } }

        rule check_replacements_before_post_replacements() -> ()
        = {? if state.inline_ctx.substitutions.precedes(
            &Substitution::Replacements,
            &Substitution::PostReplacements,
        ) { Ok(()) } else { Err("replacements do not precede post_replacements") } }

        rule check_hardbreaks() -> ()
        = {? if state.inline_ctx.rules.contains(InlineRules::HARD_BREAKS) { Ok(()) } else { Err("hard breaks disabled") } }

        rule check_quotes() -> ()
        = {? if state.inline_ctx.substitutions.enabled(&Substitution::Quotes) { Ok(()) } else { Err("quotes disabled") } }

        // A later empty expansion leaves a valid quote body, unlike literal
        // adjacent delimiters. Callers also check for their closing delimiter.
        rule empty_quote_content()
        = position:position!() {?
            (state.inline_ctx.substitutions.precedes(&Substitution::Quotes, &Substitution::Attributes)
                && state.empty_attribute_offsets.binary_search(&position).is_ok())
                .then_some(()).ok_or("quote content was already empty")
        }

        rule check_catalog_escape()
        = position:position!() {? catalog_escape_allowed(state, position).then_some(()).ok_or("escape belongs to a later substitution") }

        // This position follows the backslash. An attribute expanded after
        // macros cannot introduce an escape for that earlier substitution.
        rule check_macro_escape()
        = position:position!() {?
            macro_token_allowed(state, position.saturating_sub(1), 1)
                .then_some(()).ok_or("escape introduced after macros")
        }

        rule check_quote_markers(open: (usize, usize), close: (usize, usize)) -> ()
        = {?
            if structural_token_allowed(state, &Substitution::Quotes, open.0, open.1)
                && structural_token_allowed(state, &Substitution::Quotes, close.0, close.1)
            {
                Ok(())
            } else {
                Err("formatting markers were introduced after quote substitution")
            }
        }

        rule inline_autolink() -> InlineNode<'input>
        = url_info:(
            "<" url:url() ">" { (url, true) }
            / "<" url:email_address() ">" { (Cow::Owned(format!("mailto:{url}")), true) }
            / url:bare_url(false) { (url, false) }
            / email_at_sign_ahead() url:email_address() { (Cow::Owned(format!("mailto:{url}")), false) }
        )
        {?
            let (url, bracketed) = url_info;
            tracing::debug!("Found autolink inline");
            let interned_url = state.intern_cow(url);
            let url_source = Source::from_str_borrowed(interned_url).map_err(|_| "failed to parse autolink URL")?;
            Ok(InlineNode::Macro(InlineMacro::Autolink(Autolink {
                url: url_source,
                bracketed,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                hide_uri_scheme: state.document_attributes.contains_key("hide-uri-scheme"),
            })))
        }

        /// Match inline autolink without consuming - for use in negative lookaheads.
        /// Uses existing sub-rules (url, bare_url, email_address) for correctness;
        /// avoids Source::from_str_borrowed and Autolink struct allocation.
        rule inline_autolink_match(escaped: bool)
        = "<" url() ">"
        / "<" email_address() ">"
        / bare_url(escaped)
        / email_at_sign_ahead() email_address()

        rule replacement_consumes_eol()
        = check_replacements_before_post_replacements() eol() "--" (" " / eol() / ![_])

        rule line_break_eol()
        = !replacement_consumes_eol() eol()

        /// End of a hard line break: either a newline, or — only in a top-level
        /// block-content parse — end of input. The block-level gate keeps a
        /// nested span ending in ` +` (e.g. `` `code +` ``, a footnote/link)
        /// literal, since `process_inlines` re-parses that inner content and
        /// would otherwise see its trailing ` +` as ending the input.
        rule inline_line_break_end()
        = line_break_eol()
        / ![_] {? if state.inline_ctx.rules.contains(InlineRules::EOI_HARD_BREAK) { Ok(()) } else { Err("hard line break at EOI requires block level") } }

        rule inline_line_break() -> InlineNode<'input>
        = " +" end:position!() inline_line_break_end()
        {?
            // `end` captures position before the trailing eol so the line break's
            // location doesn't include the newline.
            if !has_inline_line_break_prefix(state, span_start) {
                return Err("hard line break requires preceding content or an empty attribute");
            }

            tracing::debug!("Found inline line break");
            Ok(InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(span_start, end, state.inline_ctx.offset),
            }))
        }

        /// Match inline line break without consuming - for use in negative lookaheads.
        rule inline_line_break_match()
        = " +" inline_line_break_end()
        {?
            if has_inline_line_break_prefix(state, span_start) {
                Ok(())
            } else {
                Err("hard line break requires preceding content or an empty attribute")
            }
        }

        rule automatic_line_break() -> InlineNode<'input>
        = line_break_eol()
        {
            InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            })
        }

        rule hard_wrap() -> InlineNode<'input>
            = " + \\" &eol()
        {
            tracing::debug!("Found hard wrap inline");
            // The trailing `&eol()` is a zero-width lookahead, so `span_end` matches
            // the position right after `\` (before the newline).
            InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            })
        }

        /// Match hard wrap without consuming - for use in negative lookaheads.
        rule hard_wrap_match()
        = " + \\" &eol()

        rule inline_icon() -> InlineNode<'input>
        = "icon:" source:source() attributes:macro_attributes()
        {
            let (_discrete, metadata, _title_position) = attributes;
            let mut metadata = metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            // For font mode, the first positional (style) can be a size value (1x, 2x,
            // lg, fw) -> stored as "size" attribute;
            //
            // For image mode, the first positional (style) can be alt text.
            if let Some(style) = metadata.style.take() {
                let style_value = strip_quotes(style).to_owned();
                if ICON_SIZES.contains(&style_value.as_str()) {
                    // Named size= attribute takes precedence over positional size so we
                    // insert rather than set (set overrides).
                    metadata.attributes.insert(
                        "size".into(),
                        AttributeValue::String(Cow::Owned(style_value)),
                    );
                } else {
                    // Other value become alt (fa-{value} in image mode)
                    metadata.attributes.set(
                        "alt".into(),
                        AttributeValue::String(Cow::Owned(style_value)),
                    );
                }
            }
            // Copy roles to attributes so they're accessible in the converter
            if !metadata.roles.is_empty() {
                metadata.attributes.set(
                    "role".into(),
                    AttributeValue::String(metadata.roles.join(" ").into()),
                );
            }
            InlineNode::Macro(InlineMacro::Icon(Icon {
                target: source,
                attributes: metadata.attributes.clone(),
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline icon without consuming - for use in negative lookaheads.
        /// Uses source()/macro_attributes() for correctness; avoids the heavy
        /// action-block processing (metadata manipulation, attribute copying).
        rule inline_icon_match()
        = "icon:" source() "[" (!"]" [_])* "]"

        rule inline_stem() -> InlineNode<'input>
        = prefix:$("latexmath" / "asciimath" / "stem") ":[" content:escaped_bracket_content() "]"
        {
            let notation = match prefix {
                "latexmath" => StemNotation::Latexmath,
                "asciimath" => StemNotation::Asciimath,
                _ => {
                    // stem:[] — resolve from :stem: document attribute
                    match state.document_attributes.text("stem") {
                        Some(s) => StemNotation::from_str(s).unwrap_or(StemNotation::Asciimath),
                        _ => StemNotation::Asciimath,
                    }
                }
            };

            InlineNode::Macro(InlineMacro::Stem(Stem {
                content,
                notation,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline stem without consuming - for use in negative lookaheads.
        /// Replicates escaped_bracket_content matching pattern (handles \] escapes).
        rule inline_stem_match()
        = ("latexmath" / "asciimath" / "stem") ":[" ("\\\\" / "\\" ['[' | ']'] / [^(']' | '\\')]+ / "\\")* "]"

        rule inline_image() -> InlineNode<'input>
        = "image:" source:media_source() attributes:image_macro_attributes()
        {?
            let (_discrete, metadata, title_position) = attributes;
            let mut metadata = metadata.clone();
            let mut title = Title::default();
            if let Some(style) = metadata.style.take() {
                // For inline images, the first positional attribute is the alt text (title)
                title = Title::new(vec![InlineNode::PlainText(Plain {
                    content: style,
                    location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                    escaped: false,
                })]);
            }
            let [width, height] = metadata.take_positional_attributes::<2>();
            if let Some(height) = height {
                metadata.attributes.insert("height".into(), AttributeValue::String(Cow::Borrowed(height.value)));
            }
            if let Some(width) = width {
                metadata.attributes.insert("width".into(), AttributeValue::String(Cow::Borrowed(width.value)));
            }
            metadata.move_positional_attributes_to_attributes();
            // For inline images, if there's no first positional (no alt text in title field),
            // check if there's a named title attribute. Only then should we use it to populate
            // the title field for rendering purposes, but we keep it in attributes for the
            // HTML title attribute (hover text).
            if title.is_empty()
                && metadata.attributes.get("title").is_some()
                && let Some((title_start, title_end)) = title_position
            {
                // Get the title content directly from the input to avoid lifetime issues
                // with local borrows from metadata.attributes
                let content: &'input str = &state.input[title_start..title_end];
                let bm = BlockParsingMetadata {
                    substitutions: state.inline_ctx.substitutions,
                    ..BlockParsingMetadata::default()
                };
                let title_start_pos = PositionWithOffset {
                    offset: title_start,
                    position: state.line_map.offset_to_position(title_start, state.input),
                };
                let (title_inlines, _) = process_inlines_or_err!(
                    process_inlines(state, &bm, title_start_pos.offset, title_end, state.inline_ctx.offset, content),
                    "could not process title in inline image macro"
                )?;
                title = Title::new(title_inlines);
            }
            // Note: We do NOT remove the title attribute - it's needed for the HTML title attribute

            Ok(InlineNode::Macro(InlineMacro::Image(Box::new(Image {
                title,
                source,
                metadata: metadata.clone(),
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),

            }))))
        }

        /// Match inline image without consuming - for use in negative lookaheads.
        rule inline_image_match()
        = "image:" media_source() "[" (!"]" [_])* "]"

        /// Parse a link target and its optional label or named attributes.
        rule link_macro() -> InlineNode<'input>
        = "link:" target:link_macro_source() fragment:path_fragment()? open:position!() "["
        content_start:position!() content:link_macro_content() close:position!() "]" check_link_brackets(open, close)
        {?
            tracing::debug!("Found link macro inline");
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            let target = match fragment {
                Some(f) => {
                    let combined = state.intern_fmt(format_args!("{target}{f}"));
                    Source::from_str_borrowed(combined).unwrap_or(target)
                }
                None => target,
            };
            let ProcessedLinkContent { text, attributes, .. } = process_link_content(state, &bm, content_start, span_end, &content, false)
                .map_err(|_| {
                    tracing::error!("could not process link text");
                    "could not process link text"
                })?;
            Ok(InlineNode::Macro(InlineMacro::Link(Link {
                text,
                target,
                attributes,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                hide_uri_scheme: state.document_attributes.contains_key("hide-uri-scheme"),
            })))
        }

        /// Recognize link syntax for escaping and inline lookahead.
        rule link_macro_match()
        = "link:" link_macro_source() path_fragment()? open:position!() "["
          ("\\]" / !"]" [_])* close:position!() "]" check_link_brackets(open, close)

        // Attribute values expanded after macros cannot complete a link's syntax.
        rule check_link_brackets(open: usize, close: usize)
        = {?
            (macro_token_allowed(state, open, 1) && macro_token_allowed(state, close, 1))
                .then_some(()).ok_or("link bracket introduced after macros")
        }

        // Asciidoctor's URI pass removes this escape before the named link pass.
        rule link_macro_source() -> Source<'input>
        = ("\\" &("http://" / "https://" / "ftp://" / "irc://"))? target:source() { target }

        // Complete links are tried first. An unfinished link target stays text,
        // while its remaining label can still contain formatting and other links.
        rule literal_link_target() -> InlineNode<'input>
        = content:$(literal_link_target_match()) {
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        rule literal_link_target_match()
        = "link:" ("\\"* url_macro_target_match()
            / "mailto:" email_address()
            / email_at_sign_ahead() email_address())
        // Local targets need recovery only when lookahead accepts a closer that
        // belongs to a nested macro. Other unfinished paths stay ordinary text.
        / &link_macro_match() "link:" link_macro_source() path_fragment()?

        /// Parse cross-reference shorthand syntax: <<id>> or <<id,custom text>>
        rule cross_reference_shorthand() -> InlineNode<'input>
        = shorthand:cross_reference_shorthand_pattern()
        {?
            let (target, raw_text) = shorthand;
            let target_str: &'input str = target;
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            // Shorthand-specific pre-trim: asciidoctor treats whitespace-only
            // custom text in `<<id, >>` as "no custom text" and falls back to
            // the target's section title. We drop to `vec![]` here so the
            // converter hits its fallback branch. This diverges from the
            // `xref:id[ ]` macro form, where `process_inlines_no_autolinks`
            // preserves the whitespace literally — matching asciidoctor's
            // asymmetry between the two syntaxes.
            let text = if let Some((content_start, t)) = raw_text {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    vec![]
                } else {
                    let content_pos = PositionWithOffset {
                        offset: content_start,
                        position: state.line_map.offset_to_position(content_start, state.input),
                    };
                    process_inlines_no_autolinks(state, &bm, content_pos.offset, span_end, state.inline_ctx.offset, trimmed)
                        .map_err(|_| {
                            tracing::error!("could not process xref text");
                            "could not process xref text"
                        })?
                }
            } else {
                vec![]
            };
            tracing::debug!("Found cross-reference shorthand");
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            let mut xref = crate::CrossReference::new(target_str, location).with_text(text);
            xref.resolve_natural_target = !state.document_attributes.contains_key("compat-mode");
            if xref.resolve_natural_target {
                xref.source_syntax = crate::model::XrefSourceSyntax::Shorthand;
            }
            xref.target_is_local = xref.source_syntax.target_is_local(xref.target);
            xref.xrefstyle = crate::XrefStyle::from_attribute(
                state
                    .document_attributes
                    .get("xrefstyle")
                    .map(|value| value.text().unwrap_or_default())
                    .map(crate::strip_quotes),
            );
            if xref.text.is_empty() {
                xref.caption_label_snapshot_id = Some(state.capture_xref_caption_labels());
            }
            Ok(InlineNode::Macro(InlineMacro::CrossReference(xref)))
        }

        /// Pattern for cross-reference shorthand: <<id>> or <<reference text,custom text>>
        rule cross_reference_shorthand_pattern() -> (&'input str, Option<(usize, &'input str)>)
        = "<<" target:$((!("," / ">>") [_])+) content:("," content_start:position!() text:$((!">>" [_])+) { (content_start, text) })? ">>"
        {?
            if target
                .chars()
                .next()
                .is_some_and(|character| character.is_alphanumeric() || matches!(character, '_' | '#' | '/' | '.' | ':' | '{'))
            {
                Ok((target, content))
            } else {
                Err("invalid first character in cross-reference shorthand")
            }
        }

        /// Parse cross-reference macro syntax: xref:id[text] or xref:file.adoc#anchor[text]
        rule cross_reference_macro() -> InlineNode<'input>
        = "xref:" target_str:xref_target() "[" content_start:position!() raw_text:cross_reference_macro_text() "]"
        {?
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            let macro_text = xref_macro_text(
                raw_text,
                state.document_attributes.contains_key("compat-mode"),
            );
            let text = if macro_text.text.is_empty() {
                vec![]
            } else {
                let rules = state.inline_ctx.rules;
                state.inline_ctx.rules.insert(InlineRules::BRACKET_LABEL);
                state.inline_ctx.rules.remove(InlineRules::QUOTED_LINK);
                let parsed = match macro_text.text {
                    Cow::Borrowed(text) => {
                        // Brackets read as written keep the span they always had.
                        let start = content_start + macro_text.offset;
                        let end = if macro_text.as_written { span_end } else { start + text.len() };
                        process_inlines_no_autolinks(state, &bm, start, end, state.inline_ctx.offset, text)
                    }
                    Cow::Owned(text) => {
                        let start = content_start + macro_text.offset + state.inline_ctx.offset;
                        let text = state.intern_str(&text);
                        process_unescaped_xref_label(state, &bm, text, start, &macro_text.source_map)
                    }
                };
                state.inline_ctx.rules = rules;
                parsed
                    .map_err(|_| {
                        tracing::error!("could not process xref text");
                        "could not process xref text"
                    })?
            };
            tracing::debug!("Found cross-reference macro");
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            let mut xref = crate::CrossReference::new(target_str, location).with_text(text);
            if !state.document_attributes.contains_key("compat-mode") {
                xref.source_syntax = crate::model::XrefSourceSyntax::Macro;
            }
            xref.target_is_local = xref.source_syntax.target_is_local(xref.target);
            // A per-reference `xrefstyle=` wins over the document's, including
            // an unrecognised one, which Asciidoctor treats as `basic`.
            xref.xrefstyle = crate::XrefStyle::from_attribute(
                macro_text
                    .xrefstyle
                    .as_deref()
                    .or_else(|| state.document_attributes.get("xrefstyle").map(|value| value.text().unwrap_or_default()))
                    .map(crate::strip_quotes),
            );
            xref.role = macro_text.role.as_deref().map(|role| state.intern_str(role));
            if xref.text.is_empty() {
                xref.caption_label_snapshot_id = Some(state.capture_xref_caption_labels());
            }
            Ok(InlineNode::Macro(InlineMacro::CrossReference(xref)))
        }

        /// Parse explicit xref text without balancing arbitrary brackets.
        ///
        /// Escaped closing brackets and supported nested macros belong to the label.
        /// Other closing brackets end the xref.
        rule cross_reference_macro_text() -> &'input str
        = text:$(cross_reference_macro_text_part()*) { text }

        rule cross_reference_macro_text_part()
        = "\\]"
        / &['i'] check_macros() check_index_terms() index_term_match()
        / bracket_label_macro_match()
        / !"]" [_]

        rule bracket_label_macro_match()
        = &['p'] inline_pass_match()
        / check_macros() (
            &['[' | 'a'] inline_anchor_match()
            / &['a' | 's'] inline_stem_match()
            / &['b'] inline_button_match()
            / &['f'] footnote_match() {}
            / &['f' | 'h'] url_macro_match(false)
            / &['i'] (
                inline_image_match()
                / inline_icon_match()
                / url_macro_match(false)
            )
            / &['k'] inline_keyboard_match()
            / &['l'] (inline_stem_match() / link_macro_match())
            / &['m'] (inline_menu_match() / mailto_macro_match())
        )

        /// Match cross-reference shorthand syntax without consuming.
        rule cross_reference_shorthand_match() -> ()
        = cross_reference_shorthand_pattern() {}

        /// Match cross-reference macro syntax without consuming: xref:id[text] or xref:file.adoc#anchor[text]
        rule cross_reference_macro_match()
        = "xref:" xref_target() "[" cross_reference_macro_text() "]"

        rule bold_text_unconstrained() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "**" content_start:position!() content:$(empty_quote_content() &"**" / (!"**" [_])+) close:position!() "**" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start, end, offset = state.inline_ctx.offset, "Found unconstrained bold text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process unconstrained bold text content"
            )?;
            Ok(InlineNode::BoldText(Bold {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match unconstrained bold without consuming - for use in negative lookaheads.
        rule bold_text_unconstrained_match()
        = inline_attributes()? open:position!() "**" (empty_quote_content() &"**" / (!"**" [_])+) close:position!() "**" check_quote_markers((open, 2), (close, 2))

        /// A different non-word character or end of input closes constrained formatting.
        /// Formatting marks are punctuation too; an underscore remains a word
        /// character. A repeated marker stays inside its delimiter run. Consuming
        /// the boundary supports both `&` and `!` lookaheads.
        rule constrained_boundary_follow(marker: char)
        = (position:position!() {? (marker == '`' && code_followed_by_attribute(state, position))
            .then_some(()).ok_or("code followed by an original attribute reference") }) ([_] / ![_])
        / !['a'..='z' | 'A'..='Z' | '0'..='9' | '_'] c:['\0'..='\x7f'] {?
            if c == marker { Err("same formatting marker") } else { Ok(()) }
        }
        / non_word_non_ascii_char()
        / ![_]

        /// A single non-ASCII character that is not a Unicode word character
        /// (letter or number).
        rule non_word_non_ascii_char()
        = c:$([_]) {?
            match c.chars().next() {
                Some(ch) if !ch.is_ascii() && !ch.is_alphanumeric() => Ok(()),
                _ => Err("not a non-word non-ASCII character"),
            }
        }

        // A complete link owns its label's markers. Recognition has no catalog
        // side effects; its content is parsed only after the outer span succeeds.
        rule formatting_link_match()
        = &['f' | 'h' | 'i' | 'l' | 'm' | 'x' | '<'] check_macros() (
            (url_macro_target_match()
                / "link:" link_macro_source() path_fragment()?
                / "mailto:" email_address() ("?" url_path_char()*)?)
                "[" link_macro_content_part()* "]"
            / cross_reference_macro_match() / cross_reference_shorthand_match()
        )

        rule constrained_formatting_edge(content_position: usize)
        = edge:position!() {?
            let whitespace = state.input.as_bytes().get(content_position)
                .is_some_and(|byte| matches!(byte, b' ' | b'\t'..=b'\r'));
            // Before attribute expansion, the edge was a reference rather than whitespace.
            (!whitespace || (state.inline_ctx.substitutions.precedes(&Substitution::Quotes, &Substitution::Attributes)
                && (byte_came_from_attribute(state, content_position)
                    || state.empty_attribute_offsets.binary_search(&edge).is_ok())))
                .then_some(()).ok_or("whitespace at constrained content edge")
        }

        // Reject invalid closers while scanning so a later marker can close the span.
        rule constrained_formatting_close(marker: char)
        = start:position!() constrained_formatting_edge(start.saturating_sub(1))
        c:[_] next:position!() &constrained_boundary_follow(marker) {?
            (c == marker && (c != '`' || code_followed_by_attribute(state, next)
                || !matches!(state.input.as_bytes().get(next), Some(b'"' | b'\''))))
                .then_some(()).ok_or("constrained formatting delimiter")
        }

        rule constrained_formatting_content(marker: char)
        = empty_quote_content() &constrained_formatting_close(marker)
        / start:position!() constrained_formatting_edge(start) (formatting_link_match() / [_])
          (formatting_link_match() / !constrained_formatting_close(marker) [_])*

        rule bold_text_constrained() -> InlineNode<'input>
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "*"
        content:$constrained_formatting_content('*')
        close:position!() "*" check_quote_markers((start, 1), (close, 1))
        end:position!() &constrained_boundary_follow('*')
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + state.inline_ctx.offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'*') {
                tracing::debug!(absolute_pos, "Invalid word boundary for constrained bold");
                return Err("invalid word boundary for constrained bold");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained bold");
            }

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(offset = state.inline_ctx.offset, "Found constrained bold text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'*');
            let result = process_inlines_or_err!(
                process_inlines(state, &bm, adjusted_content_start.offset, end - 1, state.inline_ctx.offset, content),
                "could not process constrained bold text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let (content, _) = result?;

            Ok(InlineNode::BoldText(Bold {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        rule bold_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        open:position!() "*"
        constrained_formatting_content('*')
        close:position!() "*" check_quote_markers((open, 1), (close, 1))
        closing_pos:position!()
        constrained_boundary_follow('*')
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'*');
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_constrained() -> InlineNode<'input>
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "_"
        content:$constrained_formatting_content('_')
        close:position!() "_" check_quote_markers((start, 1), (close, 1))
        end:position!() &constrained_boundary_follow('_')
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + state.inline_ctx.offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'_') {
                return Err("invalid word boundary for constrained italic");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained italic");
            }

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(offset = state.inline_ctx.offset, "Found constrained italic text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'_');
            let result = process_inlines_or_err!(
                process_inlines(state, &bm, adjusted_content_start.offset, end - 1, state.inline_ctx.offset, content),
                "could not process constrained italic text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let (content, _) = result?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        rule italic_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        open:position!() "_"
        constrained_formatting_content('_')
        close:position!() "_" check_quote_markers((open, 1), (close, 1))
        closing_pos:position!()
        constrained_boundary_follow('_')
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'_');
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_unconstrained() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "__" content_start:position!() content:$(empty_quote_content() &"__" / (!"__" [_])+) close:position!() "__" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start, end, offset = state.inline_ctx.offset, "Found unconstrained italic text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process unconstrained italic text content"
            )?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match unconstrained italic without consuming - for use in negative lookaheads.
        rule italic_text_unconstrained_match()
        = inline_attributes()? open:position!() "__" (empty_quote_content() &"__" / (!"__" [_])+) close:position!() "__" check_quote_markers((open, 2), (close, 2))

        rule monospace_text_unconstrained() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "``" content_start:position!() content:$(empty_quote_content() &"``" / (!"``" [_])+) close:position!() "``" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start, end, offset = state.inline_ctx.offset, "Found unconstrained monospace text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process unconstrained monospace text content"
            )?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match unconstrained monospace without consuming - for use in negative lookaheads.
        rule monospace_text_unconstrained_match()
        = inline_attributes()? open:position!() "``" (empty_quote_content() &"``" / (!"``" [_])+) close:position!() "``" check_quote_markers((open, 2), (close, 2))

        // Reserve backticks beside quotes for curved quotation syntax.
        rule monospace_boundary_follow()
        = (position:position!() {? code_followed_by_attribute(state, position)
            .then_some(()).ok_or("code followed by an original attribute reference") }) ([_] / ![_])
        / !['"' | '\''] constrained_boundary_follow('`')

        rule monospace_text_constrained() -> InlineNode<'input>
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "`"
        content:$constrained_formatting_content('`')
        close:position!() "`" check_quote_markers((start, 1), (close, 1))
        end:position!()
        &monospace_boundary_follow()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + state.inline_ctx.offset;
            if !check_code_opening_boundary(state, absolute_pos) {
                return Err("monospace must be at word boundary");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained monospace");
            }

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start = content_start.offset, end, offset = state.inline_ctx.offset, "Found constrained monospace text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'`');
            let result = process_inlines_or_err!(
                process_inlines(state, &bm, adjusted_content_start.offset, end - 1, state.inline_ctx.offset, content),
                "could not process constrained monospace text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let (content, _) = result?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        rule monospace_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        open:position!() "`"
        constrained_formatting_content('`')
        close:position!() "`" check_quote_markers((open, 1), (close, 1))
        closing_pos:position!()
        monospace_boundary_follow()
        {?
            let valid_opening = check_code_opening_boundary(state, boundary_pos);
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("monospace must be at word boundary") }
        }

        rule highlight_text_unconstrained() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "##" content_start:position!() content:$(empty_quote_content() &"##" / (!"##" [_])+) close:position!() "##" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start, end, offset = state.inline_ctx.offset, "Found unconstrained highlight text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process unconstrained highlight text content"
            )?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match unconstrained highlight without consuming - for use in negative lookaheads.
        rule highlight_text_unconstrained_match()
        = inline_attributes()? open:position!() "##" (empty_quote_content() &"##" / (!"##" [_])+) close:position!() "##" check_quote_markers((open, 2), (close, 2))

        rule highlight_text_constrained() -> InlineNode<'input>
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "#"
        content:$constrained_formatting_content('#')
        close:position!() "#" check_quote_markers((start, 1), (close, 1))
        end:position!()
        &constrained_boundary_follow('#')
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + state.inline_ctx.offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'#') {
                tracing::debug!(absolute_pos, "Invalid word boundary for constrained highlight");
                return Err("invalid word boundary for constrained highlight");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained highlight");
            }

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start = content_start.offset, end, offset = state.inline_ctx.offset, "Found constrained highlight text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'#');
            let result = process_inlines_or_err!(
                process_inlines(state, &bm, adjusted_content_start.offset, end - 1, state.inline_ctx.offset, content),
                "could not process constrained highlight text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let (content, _) = result?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        rule highlight_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        open:position!() "#"
        constrained_formatting_content('#')
        close:position!() "#" check_quote_markers((open, 1), (close, 1))
        closing_pos:position!()
        constrained_boundary_follow('#')
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'#');
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("invalid word boundary") }
        }

        /// Parse superscript text (^text^)
        rule superscript_text() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "^" content_start:position!() content:$(empty_quote_content() &"^" / [^('^' | ' ' | '\t' | '\n')]+) close:position!() "^" check_quote_markers((start, 1), (close, 1)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start, end, offset = state.inline_ctx.offset, "Found superscript text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 1, state.inline_ctx.offset, content),
                "could not process superscript text content"
            )?;
            Ok(InlineNode::SuperscriptText(Superscript {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match superscript text without consuming - for use in negative lookaheads.
        rule superscript_text_match()
        = inline_attributes()? open:position!() "^" (empty_quote_content() &"^" / [^('^' | ' ' | '\t' | '\n')]+) close:position!() "^" check_quote_markers((open, 1), (close, 1))

        /// Parse subscript text (~text~)
        rule subscript_text() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "~" content_start:position!() content:$(empty_quote_content() &"~" / [^('~' | ' ' | '\t' | '\n')]+) close:position!() "~" check_quote_markers((start, 1), (close, 1)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start, end, offset = state.inline_ctx.offset, "Found subscript text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 1, state.inline_ctx.offset, content),
                "could not process subscript text content"
            )?;
            Ok(InlineNode::SubscriptText(Subscript {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match subscript text without consuming - for use in negative lookaheads.
        rule subscript_text_match()
        = inline_attributes()? open:position!() "~" (empty_quote_content() &"~" / [^('~' | ' ' | '\t' | '\n')]+) close:position!() "~" check_quote_markers((open, 1), (close, 1))

        /// Parse curved quotation text (`"text"`)
        rule curved_quotation_text() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "\"`" content_start:position!() content:$(empty_quote_content() &"`\"" / (!("`\"") [_])+) close:position!() "`\"" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start, end, offset = state.inline_ctx.offset, "Found curved quotation text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process curved quotation text content"
            )?;
            Ok(InlineNode::CurvedQuotationText(CurvedQuotation {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match curved quotation text without consuming - for use in negative lookaheads.
        rule curved_quotation_text_match()
        = inline_attributes()? open:position!() "\"`" (empty_quote_content() &"`\"" / (!("`\"") [_])+) close:position!() "`\"" check_quote_markers((open, 2), (close, 2))

        /// Parse curved apostrophe text (`'text'`)
        rule curved_apostrophe_text() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "'`" content_start:position!() content:$(empty_quote_content() &"`'" / (!("`'") [_])+) close:position!() "`'" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(start, content_start, end, offset = state.inline_ctx.offset, "Found curved apostrophe text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process curved apostrophe text content"
            )?;
            Ok(InlineNode::CurvedApostropheText(CurvedApostrophe {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match curved apostrophe text without consuming - for use in negative lookaheads.
        rule curved_apostrophe_text_match()
        = inline_attributes()? open:position!() "'`" (empty_quote_content() &"`'" / (!("`'") [_])+) close:position!() "`'" check_quote_markers((open, 2), (close, 2))

        /// Match standalone curved apostrophe without consuming - for use in negative lookaheads.
        rule standalone_curved_apostrophe_match()
        = start:position!() "`'" check_quote_markers((start, 2), (start, 2))

        /// Parse standalone curved apostrophe (`')
        rule standalone_curved_apostrophe() -> InlineNode<'input>
            = start:position!() "`'" check_quote_markers((start, 2), (start, 2))
        {?
            tracing::debug!(start = span_start, end = span_end, offset = state.inline_ctx.offset, "Found standalone curved apostrophe inline");
            Ok(InlineNode::StandaloneCurvedApostrophe(StandaloneCurvedApostrophe {
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        rule warn_anchor_id_with_whitespace() -> ()
        = &(
            id:$([^'\'' | ',' | ']' | '.' | '#']+)
            {?
                if id.chars().any(char::is_whitespace) {
                    let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
                    state.add_generic_warning_at(
                        format!("anchor id '{id}' contains whitespace which is not allowed, treating as literal text"),
                        location,
                    );
                }
                // Always fail so the lookahead doesn't match - we just want the side
                // effect
                Err::<(), &'static str>("")
            }
        )

        // Consume only one escape and only for complete syntax. Invalid or disabled
        // anchor macros must retain their backslash instead of using the generic escape.
        rule escaped_anchor_macro() -> InlineNode<'input>
        = "\\" content:$(anchor_macro_pattern()) {
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        rule anchor_macro() -> InlineNode<'input>
        = parts:anchor_macro_pattern() {
            let (id, text) = parts;
            let reftext = (!text.is_empty()).then(|| {
                if text.contains("\\]") {
                    state.intern_str(&text.replace("\\]", "]"))
                } else {
                    text
                }
            });
            InlineNode::InlineAnchor(
                Anchor::new(id, state.create_block_location(span_start, span_end, state.inline_ctx.offset))
                    .with_xreflabel(reftext),
            )
        }

        // Share recognition with lookaheads so plain text and xref labels stop at
        // exactly the syntax that creates an anchor. Labels are text, not attributes.
        rule anchor_macro_pattern() -> (&'input str, &'input str)
        = start:position!() "anchor:" id:anchor_macro_id() "["
          open_end:position!() check_anchor_macro_token(start, open_end - start)
          text:$(("\\]" / !anchor_macro_close() !eol() [_])*) anchor_macro_close()
        { (id, text) }

        // Keep combining marks and other non-ASCII ID characters without category
        // tables. This is broader than Asciidoctor's word class; ASCII syntax,
        // whitespace, controls, and the first character remain constrained.
        rule anchor_macro_id() -> &'input str
        = id:$(anchor_id_start_char()
          (['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | ':' | '.']
          / non_ascii_id_char())*) { id }

        rule anchor_id_start_char()
        = ['_' | ':'] / c:[_] {? c.is_alphabetic().then_some(()).ok_or("anchor ID start") }

        rule non_ascii_id_char()
        = c:['\u{80}'..='\u{10FFFF}'] {?
            (!c.is_whitespace() && !c.is_control())
                .then_some(()).ok_or("anchor ID character")
        }

        rule check_anchor_macro_token(start: usize, len: usize)
        = {? macro_token_allowed(state, start, len).then_some(()).ok_or("anchor syntax introduced after macros") }

        rule anchor_macro_close()
        = start:position!() "]" check_anchor_macro_token(start, 1)

        rule inline_anchor() -> InlineNode<'input>
        = anchor_macro()
        / start:position!() double_open_square_bracket()
        // Whitespace is excluded - IDs must not contain spaces
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(
            comma() reftext:anchor_reftext(start) {
                Some(reftext)
            } /
            {
                None
            }
        )
        double_close_square_bracket()
        {
            let substituted_id = state.intern_cow(substitute(id, HEADER, &state.document_attributes));
            let substituted_reftext = reftext.map(|rt| state.intern_cow(substitute(rt, HEADER, &state.document_attributes)));
            InlineNode::InlineAnchor(Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                bibliography_label: None,
                bibliography: false,
            })
        }

        rule inline_anchor_match() -> ()
        = anchor_macro_pattern() {}
        / start:position!() double_open_square_bracket() [^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+ (comma() anchor_reftext(start))? double_close_square_bracket()

        rule bibliography_anchor_start(start: usize)
        = {?
            start.checked_sub(1)
                .and_then(|offset| state.input.get(offset..start))
                .filter(|previous| *previous == "[")
                .map(|_| ())
                .ok_or("not a bibliography anchor")
        }

        rule anchor_reftext(start: usize) -> &'input str
        // Reserve the last three closing brackets for the bibliography delimiter.
        = bibliography_anchor_start(start)
          label:$((!("]]]" !"]") [^'\n' | '\r'])+) { label }
        / !bibliography_anchor_start(start) label:$([^']']+) { label }

        rule invalid_bibliography_anchor() -> InlineNode<'input>
        = syntax:$("[[[" [^']' | '\n']* "]]]") {?
            let body = &syntax[3..syntax.len() - 3];
            let id = body.split_once(',').map_or(body, |(id, _)| id);
            if is_valid_bibliography_id(id) {
                Err("valid bibliography anchor")
            } else {
                Ok(InlineNode::PlainText(Plain {
                    content: syntax,
                    location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                    escaped: false,
                }))
            }
        }

        /// Rust-native guard: returns Ok for characters that cannot start any inline
        /// construct. Uses the static `PLAIN_TEXT_SAFE` lookup table plus context
        /// checks for email (`@` within 64 bytes) and hard wrap (space followed by `+`).
        rule plain_text_quick_safe() -> ()
        = pos:position!() {?
            let b = state.input.as_bytes().get(pos).copied().unwrap_or(0);
            if !is_plain_text_safe(b) {
                return Err("needs full check");
            }
            if b == b' ' {
                if state.input.as_bytes().get(pos + 1).copied() == Some(b'+') {
                    return Err("potential hard wrap");
                }
                return Ok(());
            }
            if b.is_ascii_alphanumeric() && has_at_sign_ahead(state, pos) {
                return Err("potential email");
            }
            Ok(())
        }

        rule plain_text() -> InlineNode<'input>
        = start_pos:position!()
        content:$((
            // Escape sequences for superscript/subscript markers - only when NOT followed by
            // a complete pattern (those are handled by escaped_superscript_subscript rule)
            "\\" "^" !([^'^' | ' ' | '\t' | '\n']+ "^")
            / "\\" "~" !([^'~' | ' ' | '\t' | '\n']+ "~")
            // Fast path: characters that can never start any inline construct.
            / ['\t' | ',' | ';' | '.' | '?' | '!' | ':' | '/' | '>' | ')' | ']' | '}' | '|' | '@' | '&' | '=' | '{' | '-' | '\u{00A0}'..='\u{FFFC}' | '\u{FFFE}'..='\u{10FFFF}']+
            // Quick path: Rust-native lookup table check for safe characters (uppercase,
            // non-macro lowercase, digits, space). Skips the full 7-branch PEG lookahead.
            / plain_text_quick_safe() [_]
            // Slow path: potential construct trigger character. Use character-class guards to
            // skip groups of rules whose starting character doesn't match.
            / (
                !(
                    // Attribute expansion can create an internal empty line. Let
                    // plain text absorb it when post replacements cannot handle it.
                    check_post_replacements() paragraph_break()
                    / profiled_attribute_match()
                    / check_hardbreaks() line_break_eol()
                    / ![_]
                    / &['\\'] (check_macros() "\\" anchor_macro_pattern() / index_label_bracket_match() / escaped_label_character_match() / escaped_syntax_match())
                    / check_post_replacements() &[' '] (hard_wrap_match() / inline_line_break_match())
                    // Macro guard: [ ( < for delimiters, then first letters of each macro:
                    // a=anchor/asciimath, b=btn, f=footnote/ftp, h=http(s), i=image/icon/indexterm/irc,
                    // k=kbd, l=link/latexmath, m=menu/mailto, p=pass, s=stem, x=xref
                    / (check_macros() &['[' | '(' | '<' | 'a' | 'b' | 'f' | 'h' | 'i' | 'k' | 'l' | 'm' | 'p' | 's' | 'x'] (inline_anchor_match() / (check_index_terms() index_term_match()) / cross_reference_shorthand_match() / cross_reference_macro_match() / footnote_match() / inline_image_match() / inline_icon_match() / inline_stem_match() / inline_keyboard_match() / inline_button_match() / inline_menu_match() / mailto_macro_match() / url_macro_match(false) / inline_pass_match() / link_macro_match()))
                    / (check_macros() &['l'] literal_link_target_match())
                    / (check_macros() check_autolinks() inline_autolink_match(false))
                    / (check_quotes() &['*' | '_' | '`' | '#' | '^' | '~' | '"' | '\'' | '['] (bold_text_unconstrained_match() / bold_text_constrained_match() / italic_text_unconstrained_match() / italic_text_constrained_match() / monospace_text_unconstrained_match() / monospace_text_constrained_match() / highlight_text_unconstrained_match() / highlight_text_constrained_match() / superscript_text_match() / subscript_text_match() / curved_quotation_text_match() / curved_apostrophe_text_match() / standalone_curved_apostrophe_match()))
                ) [_]
            )
        )+)
        end:position!()
        {
            tracing::trace!(input_len = content.len(), "Found plain text inline");
            // Note: Backslash escape stripping (e.g., \^ -> ^) is handled by the converter,
            // not here, so that verbatim contexts (like monospace) preserve backslashes.
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(start_pos, end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        /// Parse optional attribute list for inline elements
        /// Returns (roles, id) extracted from attributes like [.role1.role2] or [#id.role]
        /// This is a simplified version of block attributes, used for inline formatting
        /// A bare first positional value is a role; only a leading . or # starts shorthand.
        /// In inline context, % is a literal character, not an option separator.
        rule inline_attributes() -> (Vec<&'input str>, Option<&'input str>)
        = open_square_bracket() role:bare_inline_role() [^']']* close_square_bracket()
        { (vec![role], None) }
        / open_square_bracket() whitespace()* shorthands:inline_shorthand()+ [^']']* close_square_bracket()
        {
            let mut roles: Vec<&'input str> = Vec::new();
            let mut id: Option<&'input str> = None;

            for s in shorthands {
                match s {
                    Shorthand::Role(r) => roles.push(state.intern_cow(r)),
                    Shorthand::Id(i) => {
                        // If multiple IDs are specified, last one wins
                        id = Some(state.intern_cow(i));
                    }
                }
            }

            (roles, id)
        }

        /// Parse inline .role and #id shorthand, preserving % as literal text.
        rule inline_shorthand() -> Shorthand<'input>
        = "#" id:inline_id() { Shorthand::Id(id.into()) }
        / "." role:inline_role() { Shorthand::Role(role.into()) }

        // Bare roles are positional text, not CSS identifiers. Preserve Unicode,
        // spaces and punctuation instead of silently discarding an unmatched tail.
        rule bare_inline_role() -> &'input str
        = role:$([^(',' | ']')]+) {?
            let role = role.trim();
            (!role.is_empty() && !role.starts_with(['.', '#']))
                .then_some(role).ok_or("bare inline role")
        }

        /// Role pattern for inline contexts - allows % as literal character
        rule inline_role() -> &'input str = $([^(',' | ']' | '#' | '.')]+)

        /// ID pattern for inline contexts - allows % as literal character
        rule inline_id() -> &'input str = $(anchor_id_start_char() inline_id_subsequent_char()*)
        rule inline_id_subsequent_char()
        = ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' | ':' | '%'] / non_ascii_id_char()

        /// Macro attribute parsing - simpler than block attributes.
        ///
        /// Does NOT support shorthand syntax (.role, #id, %option).
        /// Shorthands are only valid in block-level attributes, not inside macro brackets.
        ///
        /// Asciidoctor behavior:
        /// - `image::photo.jpg[.role]` -> alt=".role" (literal text, NOT a role)
        /// - `image::photo.jpg[Diablo 4 picture of Lilith.]` -> alt="Diablo 4 picture of Lilith."
        pub(crate) rule macro_attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = open_square_bracket()
              attrs:(att:macro_attribute() comma()? { att })*
              close_square_bracket()
        {
            let mut metadata = BlockMetadata::default();
            let title_position = process_attribute_list(
                attrs,
                &mut metadata,
                state,
                span_start,
                span_end,
                MacroAttributeContext::General,
            );
            // macro_attributes never sets discrete flag (that's block-level only)
            (false, metadata, title_position)
        }

        rule image_macro_attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = open_square_bracket()
              attrs:(att:image_macro_attribute() comma()? { att })*
              close_square_bracket()
        {
            let mut metadata = BlockMetadata::default();
            let title_position = process_attribute_list(
                attrs,
                &mut metadata,
                state,
                span_start,
                span_end,
                MacroAttributeContext::Image,
            );
            (false, metadata, title_position)
        }

        /// Positional value in macro attributes - allows . # % as literal characters
        /// This is the key difference from block attributes.
        rule macro_positional_value() -> Option<Cow<'input, str>>
            = quoted:inner_attribute_value() {
                let trimmed = strip_quotes(quoted);
                if trimmed.is_empty() { None } else { Some(trimmed.into()) }
            }
            / s:$([^('"' | ',' | ']' | '=')]+) {
                let trimmed = s.trim();
                if trimmed.is_empty() { None } else { Some(trimmed.into()) }
            }

        /// Named attribute or additional positional in macro context
        rule macro_attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = whitespace()* att:named_attribute() { att }
            / val:macro_positional_value() {
                val.map(|v| (v, AttributeValue::None, None))
            }

        rule image_macro_attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = whitespace()* "link" "=" start:position!() value:image_link_attribute_value() end:position!() {
                let substituted = substitute(value, &[Substitution::Attributes], &state.document_attributes);
                Some((Cow::Borrowed("link"), AttributeValue::String(substituted), Some((start, end))))
            }
            / macro_attribute()

        rule image_link_attribute_value() -> &'input str
            = value:named_attribute_value() { value }
            / &("," / "]") { "" }

        rule open_square_bracket() = "["
        rule close_square_bracket() = "]"
        rule double_open_square_bracket() = "[["
        rule double_close_square_bracket() = "]]"
        rule comma() = ","
        rule period() = "."
        rule empty_style() = ""
        rule role() -> &'input str = $([^(',' | ']' | '#' | '.' | '%')]+)

        rule attribute_name() -> &'input str = $((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+)

        rule attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = whitespace()* att:named_attribute() { att }
              / whitespace()* start:position!() att:positional_attribute_value() end:position!() {
                  let substituted = substitute(att, &[Substitution::Attributes], &state.document_attributes);
                  Some((substituted, AttributeValue::None, Some((start, end))))
              }

        rule id() -> &'input str
            = id:$((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+) { id }

        rule id_start_char() = ['A'..='Z' | 'a'..='z' | '_']

        rule block_style_id() -> &'input str = $(id_start_char() block_style_id_subsequent_char()*)

        rule block_style_id_subsequent_char() =
            ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-']

        rule named_attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = "id" "=" start:position!() id:id() end:position!()
                { Some((Cow::Borrowed(RESERVED_NAMED_ATTRIBUTE_ID), id.into(), Some((start, end)))) }
              / ("role" / "roles") "=" value:named_attribute_value()
                { Some((Cow::Borrowed(RESERVED_NAMED_ATTRIBUTE_ROLE), value.into(), None)) }
              / ("options" / "opts") "=" value:named_attribute_value()
                { Some((Cow::Borrowed(RESERVED_NAMED_ATTRIBUTE_OPTIONS), value.into(), None)) }
              / name:attribute_name() "=" start:position!() value:named_attribute_value() end:position!()
                {
                    let substituted_value = substitute(value, &[Substitution::Attributes], &state.document_attributes);
                    Some((Cow::Borrowed(name), AttributeValue::String(substituted_value), Some((start, end))))
                }

        rule named_attribute_value() -> &'input str
        = &("\"" / "'") inner:inner_attribute_value()
        {
            let trimmed = strip_quotes(inner);
            tracing::debug!("Found named attribute value (inner)");
            trimmed
        }
        / s:$([^(',' | '"' | '\'' | ']')]+)
        {
            tracing::debug!("Found named attribute value");
            s
        }

        rule positional_attribute_value() -> &'input str
        = quoted:inner_attribute_value() {
            let trimmed = strip_quotes(quoted);
            tracing::debug!("Found quoted positional attribute value");
            trimmed
        }
        / s:$([^('"' | ',' | ']' | '#' | '.' | '%')] [^(',' | ']' | '#' | '.' | '%' | '=')]*)
        {
            let trimmed = s.trim();
            tracing::debug!("Found unquoted positional attribute value");
            trimmed
        }

        rule inner_attribute_value() -> &'input str
        = s:$("\"" [^'"']* "\"") { s }
        / s:$("'" [^'\'']* "'") { s }

        /// URL rule matches both web URLs (proto://) and mailto: URLs
        pub rule url() -> Cow<'input, str> =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:url_path() { Cow::Owned(format!("{proto}://{path}")) }
        / "mailto:" email:email_address() { Cow::Owned(format!("mailto:{email}")) }

        rule media_url() -> Cow<'input, str> =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:media_url_path() { Cow::Owned(format!("{proto}://{path}")) }
        / "mailto:" email:email_address() { Cow::Owned(format!("mailto:{email}")) }

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

        /// Fast Rust-native guard: check if '@' appears within the next 64 bytes
        /// (RFC 5321 max local-part length). Prevents the expensive `email_address()`
        /// greedy scan at positions where no email can possibly start.
        rule email_at_sign_ahead() -> ()
        = pos:position!() {?
            if has_at_sign_ahead(state, pos) {
                Ok(())
            } else {
                Err("no @ sign ahead")
            }
        }

        rule email_address() -> Cow<'input, str>
        = local:$(
            // Quoted local part: "Jane Doe"@example.com
            // Quotes allow spaces and special chars in the local part (RFC 5321).
            "\"" [^'"']+ "\""
            // Unquoted local part (no spaces allowed)
            / ['a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '%' | '+' | '-']+
        )
        "@"
        // Format: alphanumeric+ (separator alphanumeric+)*
        // This ensures domain ends with alphanumeric (not . or -) and has proper structure.
        // e.g., `example.com.` -> matches `example.com`, trailing dot stays outside
        domain:$(
            ['a'..='z' | 'A'..='Z' | '0'..='9']+
            (['.' | '-'] ['a'..='z' | 'A'..='Z' | '0'..='9']+)*
        )
        {?
            // Require TLD - domain must contain at least one dot. This prevents `foo@bar`
            // from becoming a mailto link.
            if !domain.contains('.') {
                return Err("email domain must have TLD (contain a dot)");
            }

            Ok(Cow::Owned(format!("{local}@{domain}")))
        }

        /// URL target content following `://`.
        /// Supports query parameters, fragments, and percent escapes while excluding
        /// brackets that delimit the macro attributes.
        rule url_path() -> Cow<'input, str> = path:$(url_path_char()+)
        {?
            // Inline text was already processed; attribute values must not introduce passthroughs.
            let inline_state = InlinePreprocessorParserState::new(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
                matches!(state.scope, ParserScope::Document),
                true,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|_| {
                tracing::error!("could not preprocess url path");
                "could not preprocess url path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(Cow::Owned(restore_url_path(processed)))
        }

        /// URL target content for an inline media macro.
        /// Spaces must be internal to the target.
        rule media_url_path() -> Cow<'input, str> = path:$(url_path_char() (url_path_char() / internal_url_path_spaces())*)
        {?
            let inline_state = InlinePreprocessorParserState::new(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
                matches!(state.scope, ParserScope::Document),
                true,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|_| {
                tracing::error!("could not preprocess media URL path");
                "could not preprocess media URL path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(Cow::Owned(restore_url_path(processed)))
        }

        rule url_path_char() = passthrough_placeholder() / ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%' | '\\' ]
        rule internal_url_path_spaces() = [' ']+ &url_path_char()

        /// URL for bare autolinks — avoids capturing trailing sentence punctuation
        /// (., ;, !, etc.) by only consuming punctuation when more URL chars follow.
        rule bare_url(escaped: bool) -> Cow<'input, str> =
        check_url_opening(true, escaped) proto:$("https" / "http" / "ftp" / "irc") "://" path:bare_url_path()
        { Cow::Owned(format!("{proto}://{path}")) }

        /// URL path for bare autolinks. Like url_path() but:
        /// - Trailing punctuation (. , ; ! ? : ' *) only consumed when followed by more URL chars.
        /// - `)` only consumed as part of a balanced `(...)` group, preventing capture of
        ///   sentence-level parens like `(see http://example.com)`.
        rule bare_url_path() -> Cow<'input, str> = path:$(
            bare_url_safe_char()
            ( bare_url_safe_char()
            / bare_url_paren_group()
            / "("
            / bare_url_trailing_char() &bare_url_char()
            )*
        )
        {?
            let inline_state = InlinePreprocessorParserState::new(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
                matches!(state.scope, ParserScope::Document),
                true,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
                .map_err(|_| {
                    tracing::error!("could not preprocess bare url path");
                    "could not preprocess bare url path"
                })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(Cow::Owned(restore_url_path(processed)))
        }

        /// Balanced parenthesized group in a URL path.
        /// Handles nested parens: `http://example.com/wiki/Foo_(bar_(baz))`
        /// Only `)` consumed via this rule — unbalanced `)` is never captured.
        rule bare_url_paren_group()
        = "(" (bare_url_safe_char() / bare_url_trailing_char() / bare_url_paren_group() / "(")* ")"

        /// URL chars that are safe to end a bare URL — won't be confused with sentence punctuation.
        /// Excludes `(` and `)` which are handled separately via `bare_url_paren_group`.
        rule bare_url_safe_char() = passthrough_placeholder() / ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '~'
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

        // Xref targets are IDs or source paths, not media URLs. Keep their
        // punctuation intact until source syntax selects a local or external link.
        rule xref_target() -> &'input str
            = target:$((passthrough_placeholder() / path_char() / [':' | '#']) (!['[' | ' ' | '\t' | '\r' | '\n'] [_])*)
        { target }

        /// Fragment identifier for a `link:` macro.
        rule path_fragment() -> Cow<'input, str>
            = "#" fragment:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+)
        {
            Cow::Owned(format!("#{fragment}"))
        }

        /// Filesystem path accepted by inline macros.
        ///
        /// ASCII input uses a conservative filename set. Non-ASCII Unicode characters
        /// are accepted unchanged, and `{`/`}` permit `AsciiDoc` attribute substitution.
        pub rule path() -> Cow<'input, str> = path:$(path_char()+)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|_| {
                tracing::error!("could not preprocess path");
                "could not preprocess path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(processed.text)
        }

        /// Filesystem path for an inline media target.
        /// Existing percent escapes and internal spaces are preserved.
        rule media_path() -> Cow<'input, str> = path:$(media_path_char() (media_path_char() / internal_media_path_spaces())*)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|_| {
                tracing::error!("could not preprocess media path");
                "could not preprocess media path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(processed.text)
        }

        rule path_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '{' | '}' | '_' | '-' | '.' | '/' | '\\' | '\u{80}'..='\u{10FFFF}' ]
        rule media_path_char() = path_char() / "%"
        rule internal_media_path_spaces() = [' ']+ &media_path_char()

        pub rule source() -> Source<'input>
            = source:
        (
            p:passthrough_placeholder() {
                Source::Name(p)
            }
            / u:url() {?
                let interned = state.intern_cow(u);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse URL")
            }
            / p:path() {?
                let interned = state.intern_cow(p);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse path")
            }
        )
        { source }

        rule media_source() -> Source<'input>
            = source:
        (
            p:passthrough_placeholder() {
                Source::Name(p)
            }
            / u:media_url() {?
                let interned = state.intern_cow(u);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse media URL")
            }
            / p:media_path() {?
                let interned = state.intern_cow(p);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse media path")
            }
        )
        { source }

        rule passthrough_placeholder() -> &'input str
            = placeholder:$(
                "\u{FFFD}\u{FFFD}\u{FFFD}"
                digits()
                "\u{FFFD}\u{FFFD}\u{FFFD}"
            ) {
                placeholder
            }

        rule digits() = ['0'..='9']+

        // Verse stanzas share one inline body; an empty line is not a block boundary.
        rule paragraph_break()
        = eol()*<2,> {?
            if state.inline_ctx.rules.contains(InlineRules::PRESERVE_BLANK_LINES) {
                Err("verse preserves empty lines")
            } else {
                Ok(())
            }
        }

        rule whitespace() = quiet!{ " " / "\t" }
        rule eol() = quiet!{ "\n" }

        rule position() -> PositionWithOffset = offset:position!() {
            PositionWithOffset {
                offset,
                position: state.line_map.offset_to_position(offset, state.input)
            }
        }
    }
}
