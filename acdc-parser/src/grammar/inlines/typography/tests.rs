use std::ops::Range;

use proptest::prelude::*;

use super::{
    Edges, Replacement, ReplacementValue, WordContext, encoded_arrows, has_candidate, is_arrow,
    replacement_at, replacements,
};
use crate::SymbolKind;

fn replacement_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            "[a-zA-Z0-9 éλ]{0,16}",
            Just("(C)".to_owned()),
            Just("(R)".to_owned()),
            Just("(TM)".to_owned()),
            Just("...".to_owned()),
            Just("--".to_owned()),
            Just("->".to_owned()),
            Just("=>".to_owned()),
            Just("<-".to_owned()),
            Just("<=".to_owned()),
            Just("&lt;".to_owned()),
            Just("&gt;".to_owned()),
            Just("'".to_owned()),
            Just("\\".to_owned()),
            Just("\n".to_owned()),
        ],
        0..32,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn candidate_scan_matches_token_search(text in replacement_text()) {
        let expected = ["(C)", "(R)", "(TM)", "...", "--", "'", "->", "=>", "<-", "<=", "-&gt;", "=&gt;", "&lt;-", "&lt;=", r"&lt;\-&gt;", r"&lt;\=&gt;"]
            .iter().any(|token| text.contains(token));
        prop_assert_eq!(has_candidate(&text), expected);
    }

    #[test]
    fn replacement_scan_matches_character_walk(
        text in replacement_text(),
        start in any::<bool>(),
        end in any::<bool>(),
        before in prop::option::of((any::<char>(), any::<bool>())),
        after in prop::option::of((any::<char>(), any::<bool>())),
        blocked_modulus in 2usize..8,
    ) {
        let edges = Edges { start, end };
        let words = WordContext { before, after };
        let allowed = |range: Range<usize>, _: SymbolKind| !range.start.is_multiple_of(blocked_modulus);
        prop_assert_eq!(replacements(&text, edges, words, allowed), reference_replacements(&text, edges, words, allowed));
    }
}

// Keep the original character-by-character walk as an independent oracle for
// candidate skipping, including escape retries, overlapping edits, and UTF-8.
fn reference_replacements(
    text: &str,
    edges: Edges,
    words: WordContext,
    allowed: impl Fn(Range<usize>, SymbolKind) -> bool,
) -> Vec<Replacement> {
    let mut edits = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        let escaped = text.as_bytes().get(cursor) == Some(&b'\\');
        let start = cursor + usize::from(escaped);
        let Some((mut range, kind)) = replacement_at(text, start, edges, words) else {
            cursor += text[cursor..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        if escaped
            && !allowed(
                cursor..if is_arrow(kind) { range.end } else { start + 1 },
                kind,
            )
        {
            cursor = start;
            continue;
        }
        let end = range.end;
        if edits
            .last()
            .is_some_and(|previous: &Replacement| previous.range.end > range.start)
        {
            cursor = start + 1;
            continue;
        }
        if escaped {
            range.start = cursor;
        }
        let mut context = range.clone();
        let mut word_context_allowed = true;
        if matches!(
            kind,
            SymbolKind::CurvedApostrophe | SymbolKind::WordBoundedEmDash
        ) {
            word_context_allowed = (range.start != 0
                || words.before.is_none_or(|(_, allowed)| allowed))
                && (range.end != text.len() || words.after.is_none_or(|(_, allowed)| allowed));
            context.start -= text[..range.start]
                .chars()
                .next_back()
                .map_or(0, char::len_utf8);
            context.end += text[range.end..].chars().next().map_or(0, char::len_utf8);
        }
        let value = if !word_context_allowed || !allowed(context, kind) {
            ReplacementValue::Literal(range.clone())
        } else if escaped {
            ReplacementValue::Literal(start..end)
        } else {
            ReplacementValue::Symbol(kind)
        };
        encoded_arrows::append_edits(text, range, value, &mut edits, &allowed);
        cursor = edits.last().map_or(end, |edit| edit.range.end);
    }
    edits
}
