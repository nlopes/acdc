use std::rc::Rc;

use super::parse_normal_cell;
use crate::{CompatibilityMode, Options, grammar::ParserState};

type Error = Box<dyn std::error::Error>;

fn state(input: &str, mode: CompatibilityMode) -> Result<ParserState<'_>, Error> {
    let mut state = ParserState::new_for_test(input);
    state.options = Rc::new(Options::builder().with_compatibility_mode(mode).build()?);
    Rc::make_mut(&mut state.document_attributes).insert_text("name".into(), "World".into());
    Ok(state)
}

#[rstest::rstest]
fn repeated_cells_match_independent_parsing_and_semantic_effects(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for source in [
        "Ordinary text",
        "Hello *{name} _nested_* and `code`.",
        "Hello *{name}*.\n\nSecond _paragraph_.",
        "*unclosed formatting and ordinary text",
        "footnote:[note]",
        "*footnote:[note]*",
        "*footnote:[note] unclosed formatting",
        "footnote:shared[note] footnote:shared[]",
        "pass:m[footnote:[passthrough\\]]",
        "[[target]] See <<target>>.",
        "[#formatted]*text*",
        "((entry)) indexterm:[hidden]",
        "世界 *café* and pass:[<b>raw</b>] {sp}.",
        "Hello... and word--word.",
    ] {
        let mut independent = state(source, mode)?;
        let mut repeated = state(source, mode)?;
        let mut prepared = None;
        for _ in 0..100 {
            let expected = parse_normal_cell(source, &mut independent, 0, false, &mut None)?;
            let actual = parse_normal_cell(source, &mut repeated, 0, true, &mut prepared)?;
            assert_eq!(actual, expected, "{source}");
        }
        assert_eq!(
            repeated.footnote_tracker.borrow().footnotes,
            independent.footnote_tracker.borrow().footnotes,
            "{source}"
        );
        assert_eq!(
            &**repeated.warnings.borrow(),
            &**independent.warnings.borrow(),
            "{source}"
        );
    }
    Ok(())
}
