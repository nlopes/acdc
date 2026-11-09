use super::{plain_text_end, plain_text_start};
use crate::{
    Location, Passthrough, PassthroughKind, Substitution, grammar::ParserState,
    model::AttributeInlineFragment,
};

#[test]
fn ordinary_runs_preserve_complete_and_incomplete_placeholders() {
    for marker in [
        "���0���",
        "���12���",
        "���000���",
        "���999999999999999999999999���",
    ] {
        let source = format!("before {marker} after *bold*");
        let state = ParserState::new_for_test(&source);
        assert!(plain_text_start(&state, "before ".len()));
        assert_eq!(plain_text_end(&state, 0), source.len() - "*bold*".len());
    }
    for tail in ["���", "���0", "���0�", "���x���", "�0���", "世界", "\nnext"]
    {
        let source = format!("before {tail}");
        let state = ParserState::new_for_test(&source);
        assert!(!plain_text_start(&state, "before ".len()));
        assert_eq!(plain_text_end(&state, 0), "before ".len());
    }
}

#[test]
fn ordinary_runs_leave_profiled_attribute_effects_to_the_grammar() {
    let mut pass = Passthrough::new(
        Some("footnote:[note]"),
        vec![Substitution::Macros],
        PassthroughKind::AttributeRef,
        Location::default(),
    );
    pass.attribute_fragments = vec![AttributeInlineFragment {
        range: 0.."footnote:[note]".len(),
        substitutions: vec![Substitution::Macros].into_boxed_slice(),
        source: None,
    }]
    .into_boxed_slice();
    for marker in ["���0���", "���00���"] {
        let source = format!("before {marker} after");
        let mut state = ParserState::new_for_test(&source);
        state.attribute_passthroughs = vec![pass.clone()].into();
        assert_eq!(plain_text_end(&state, 0), "before ".len());
        assert!(!plain_text_start(&state, "before ".len()));
    }
}
