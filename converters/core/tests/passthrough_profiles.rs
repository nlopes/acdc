//! Lazy passthrough profiles preserve borrowed text when replacements are disabled.

use std::borrow::Cow;

use acdc_converters_core::substitutions::{
    Replacements, TextBoundaries, apply_passthrough_replacements_iter,
};
use acdc_parser::{Location, Raw, Substitution, ast::RawRef};

#[test]
fn passthrough_without_replacements_consumes_a_lazy_profile_and_keeps_the_text() {
    let text = String::from("<text> -> ... &amp;");
    let raw = Raw {
        content: &text,
        location: Location::default(),
        subs: vec![
            Substitution::SpecialChars,
            Substitution::Quotes,
            Substitution::SpecialChars,
        ],
    };
    let raw = RawRef::from(&raw);
    let mut consumed = 0;
    let profile = raw.substitutions().inspect(|_| consumed += 1);
    let result = apply_passthrough_replacements_iter(
        raw.text(),
        profile,
        &Replacements::unicode(),
        TextBoundaries::BOTH,
    );
    assert_eq!(consumed, 3);
    assert!(matches!(result, Cow::Borrowed(_)));
    assert_eq!(result.as_ref(), text);
    assert_eq!(result.as_ptr(), text.as_ptr());
}
