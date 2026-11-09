use acdc_parser::{
    Bold, DoubleQuotedSpan, FormattingForm, Highlight, Italic, Location, Monospace,
    SingleQuotedSpan, Subscript, Superscript,
    ast::{
        BoldRef, DoubleQuotedSpanRef, HighlightRef, ItalicRef, MonospaceRef, SingleQuotedSpanRef,
        SubscriptRef, SuperscriptRef,
    },
};

#[test]
fn exact_span_roles_keep_optional_presence_and_token_reads() {
    for role in [
        None,
        Some(""),
        Some(" \t\u{2003}"),
        Some("one\ttwo\u{2003}one"),
        Some("\"one\" &amp; <two>"),
    ] {
        macro_rules! check {
            ($owner:ident, $read:ident) => {{
                let mut owner =
                    $owner::new(Vec::new(), FormattingForm::Constrained, Location::default());
                owner.replace_role(role);
                let span = $read::from(&owner);
                assert_eq!(span.role(), role);
                if let Some(text) = role {
                    assert_eq!(span.role().map(str::as_ptr), Some(text.as_ptr()));
                }
                let tokens = span.roles().iter().collect::<Vec<_>>();
                assert_eq!(
                    tokens,
                    role.unwrap_or_default()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                );
                assert_eq!(span.roles().is_present(), role.is_some());
                assert_eq!(span.attributes().contains("role"), role.is_some());
                owner.replace_role(Some("updated\trole"));
                assert_eq!($read::from(&owner).role(), Some("updated\trole"));
            }};
        }
        check!(Bold, BoldRef);
        check!(Italic, ItalicRef);
        check!(Monospace, MonospaceRef);
        check!(Highlight, HighlightRef);
        check!(Subscript, SubscriptRef);
        check!(Superscript, SuperscriptRef);
        check!(DoubleQuotedSpan, DoubleQuotedSpanRef);
        check!(SingleQuotedSpan, SingleQuotedSpanRef);
    }
}

#[test]
fn role_text_keeps_the_owner_lifetime_after_the_handle_is_dropped() {
    let text = String::from("one\ttwo\u{2003}one");
    let owner = {
        let mut span = Bold::new(Vec::new(), FormattingForm::Constrained, Location::default());
        span.replace_role(Some(&text));
        span
    };
    let role = {
        let span = BoldRef::from(&owner);
        span.role()
    };
    assert_eq!(role, Some(text.as_str()));
    assert_eq!(role.map(str::as_ptr), Some(text.as_ptr()));
}
