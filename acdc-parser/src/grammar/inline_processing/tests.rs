use std::rc::Rc;

use bumpalo::Bump;
use proptest::prelude::*;

use super::{
    BlockParsingMetadata, ParserState, parse_inline_syntax, parse_processed_inlines,
    preprocess_inline_content, processed_text_as_outer,
};
use crate::{CompatibilityMode, Options};

proptest! {
    #[test]
    fn simple_text_matches_general_inline_parser(
        value in "[A-Za-z0-9][A-Za-z0-9 ,;.!?]{0,119}",
        draft in any::<bool>(),
        expanded in any::<bool>(),
        autolinks in any::<bool>(),
        verbatim in any::<bool>(),
    ) {
        let mode = if draft { CompatibilityMode::Draft } else { CompatibilityMode::Asciidoctor };
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("value", value.clone())
            .build().expect("valid options");
        let source = if expanded { "{value}" } else { &value };
        let prefix = "An earlier line.\n";
        let input = format!("{prefix}{source}");
        let arena = Bump::new();
        let mut state = ParserState::new(&input, &arena, Rc::new(options));
        let metadata = BlockParsingMetadata::new(mode);
        let (location, processed) = preprocess_inline_content(
            &mut state, prefix.len(), input.len(), 0, &input[prefix.len()..], metadata.substitutions,
        ).expect("valid text preprocessing");
        let text = processed_text_as_outer(&processed, &state);
        let actual = parse_processed_inlines(
            &processed, text, &mut state, &metadata, &location, autolinks, verbatim,
        ).expect("valid simple text");
        let expected = parse_inline_syntax(
            &processed, text, &mut state, &metadata, &location, autolinks, verbatim,
        ).expect("valid general parsing");
        prop_assert_eq!(actual, expected);
    }
}
