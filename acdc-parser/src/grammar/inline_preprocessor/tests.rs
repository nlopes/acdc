use super::*;
use crate::{AttributeValue, DocumentAttributes};

fn setup_attributes() -> DocumentAttributes<'static> {
    let mut attributes = DocumentAttributes::default();
    assert!(
        attributes
            .insert("s".into(), AttributeValue::String("link:/nonono".into()))
            .is_ok()
    );
    assert!(
        attributes
            .insert("version".into(), AttributeValue::String("1.0".into()))
            .is_ok()
    );
    assert!(
        attributes
            .insert("title".into(), AttributeValue::String("My Title".into()))
            .is_ok()
    );
    attributes
}

fn setup_state(content: &str) -> InlinePreprocessorParserState<'_> {
    // Leak a per-call arena so test states have the required lifetime.
    let arena: &'static Bump = Box::leak(Box::new(Bump::new()));
    InlinePreprocessorParserState {
        pass_found_count: Cell::new(0),
        passthroughs: RefCell::new(Vec::new()),
        current_offset: Cell::new(0),
        line_map: Rc::new(LineMap::new(content)),
        full_input: content,
        arena,
        source_map: RefCell::new(SourceMap::default()),
        input: RefCell::new(content),
        substring_start_offset: Cell::new(0),
        warnings: RefCell::new(Vec::new()),
        macros_enabled: true,
        attributes_enabled: true,
        defer_monospace: true,
        attribute_value_ranges: Vec::new(),
    }
}

#[test]
fn test_preprocess_inline_passthrough_single() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "+hello+";
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(
        result.text,
        "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}"
    );
    assert_eq!(state.pass_found_count.get(), 1);
    let passthroughs = result.passthroughs;
    assert_eq!(passthroughs.len(), 1);
    let Some(first) = passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert_eq!(first.text, Some("hello"));
    assert_eq!(first.kind, PassthroughKind::Single);
    Ok(())
}

#[test]
fn test_preprocess_inline_passthrough_double() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "++hello++";
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(
        result.text,
        "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}"
    );
    assert_eq!(result.passthroughs.len(), 1);
    let Some(first) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert_eq!(first.text, Some("hello"));
    assert_eq!(first.kind, PassthroughKind::Double);
    Ok(())
}

#[test]
fn test_preprocess_inline_passthrough_triple() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "+++hello+++";
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(
        result.text,
        "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}"
    );
    assert_eq!(result.passthroughs.len(), 1);
    let Some(first) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert_eq!(first.text, Some("hello"));
    assert_eq!(first.kind, PassthroughKind::Triple);
    Ok(())
}

#[test]
fn test_preprocess_inline_passthrough_single_plus() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "+hello+ world+";
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(
        result.text,
        "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} world+"
    );
    assert_eq!(result.passthroughs.len(), 1);
    let Some(first) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert_eq!(first.text, Some("hello"));
    assert_eq!(first.kind, PassthroughKind::Single);
    Ok(())
}

#[test]
fn test_preprocess_inline_passthrough_multiple() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "Something\n\nHere is some +*bold*+ text and ++**more bold**++ text.";
    //                 SomethingNNHere is some +*bold*+ text and ++**more bold**++ text.
    //                 0123456789012345678901234567890123456789012345678901234567890123456
    //                          1         2         3         4         5         6
    //                                         ^^^^^^^^          ^^^^^^^^^^^^^^^^^
    //                 Here is some +*bold*+ text and ++**more bold**++ text.
    //                 123456789012345678901234567890123456789012345678901234
    //                          1         2         3         4         5
    //                              ^^^^^^^^          ^^^^^^^^^^^^^^^^^
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;

    assert_eq!(
        result.text,
        "Something\n\nHere is some \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} text and \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} text."
    );

    assert_eq!(result.passthroughs.len(), 2);

    let Some(first) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert!(matches!(&first.text, Some(s) if *s == "*bold*"));
    assert_eq!(first.location.absolute_start, 24);
    assert_eq!(first.location.absolute_end, 32);
    assert_eq!(first.location.start.line, 3);
    assert_eq!(first.location.start.column, 14);
    assert_eq!(first.location.end.line, 3);
    assert_eq!(first.location.end.column, 22);

    let Some(second) = result.passthroughs.get(1) else {
        panic!("expected second passthrough");
    };
    assert!(matches!(&second.text, Some(s) if *s == "**more bold**"));
    assert_eq!(second.location.absolute_start, 42);
    assert_eq!(second.location.absolute_end, 59);
    assert_eq!(second.location.start.line, 3);
    assert_eq!(second.location.start.column, 32);
    assert_eq!(second.location.end.line, 3);
    assert_eq!(second.location.end.column, 49);
    Ok(())
}

#[test]
fn dense_escaped_passthroughs_finalize_source_map_in_order() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = r"\++plain++ ".repeat(256);
    let state = setup_state(&input);

    let result = inline_preprocessing::run(&input, &attributes, &state)?;

    assert_eq!(result.source_map.replacements.len(), 512);
    assert!(
        result
            .source_map
            .replacements
            .windows(2)
            .all(|replacements| {
                replacements[0].absolute_start <= replacements[1].absolute_start
            })
    );
    Ok(())
}

#[test]
fn test_preprocess_attribute_in_link() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "The {s}[syntax page] provides complete stuff.";
    let state = setup_state(input);

    let result = inline_preprocessing::run(input, &attributes, &state)?;

    assert_eq!(
        result.text,
        "The link:/nonono[syntax page] provides complete stuff."
    );

    // Original:  "The {s}[syntax page] provides complete stuff."
    //             012345678901234567890123456789012345678901234567890123
    // Processed: "The link:/nonono[syntax page] provides complete stuff."
    assert_eq!(result.source_map.map_position(15)?, 4); // This is still within the attribute so map it to the beginning.
    assert_eq!(result.source_map.map_position(16)?, 7); // This is after the attribute so map it to where it should be.
    assert_eq!(result.source_map.map_position(30)?, 21); // This is the `p` from `provides`.
    Ok(())
}

#[test]
fn test_preprocess_inline_in_attributes() -> Result<(), Error> {
    let attributes = setup_attributes();

    let input = "Version {version} of {title}";
    let state = setup_state(input);
    //                 0123456789012345678901234567
    //                 Version 1.0 of My Title
    //                 {version} -> 1.0 (-6 chars)
    //                 {title} -> My Title (+1 char)
    let result = inline_preprocessing::run(input, &attributes, &state)?;

    assert_eq!(result.text, "Version 1.0 of My Title");

    // Original:  "Version {version} of {title}"
    //             0123456789012345678901234567
    // Processed: "Version 1.0 of My Title"

    // Position 8 in original (start of {version}) should map to position 8 in
    // processed (start of "1.0")
    assert_eq!(result.source_map.map_position(8)?, 8);
    assert_eq!(result.source_map.map_position(15)?, 21);
    Ok(())
}

#[test]
fn test_preprocess_complex_example() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "Check the {s}[syntax page] and +this {s} won't expand+ for details.";
    //                 0123456789012345678901234
    //                           ^
    //                           {s} expands to link:/nonono (+9 chars)
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;

    assert_eq!(
        result.text,
        "Check the link:/nonono[syntax page] and \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} for details."
    );

    assert_eq!(result.passthroughs.len(), 1);
    let Some(first) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert!(matches!(
        &first.text,
        Some(s) if *s == "this {s} won't expand"
    ));

    let pos = result.source_map.map_position(10)?; // Start of {s}
    assert_eq!(pos, 10); // Should map to start of "link:/nonono"
    Ok(())
}

#[test]
fn test_nested_passthrough_with_nested_attributes() -> Result<(), Error> {
    let mut attributes = setup_attributes();
    assert!(
        attributes
            .insert("nested1".into(), AttributeValue::String("{version}".into()))
            .is_ok()
    );
    assert!(
        attributes
            .insert("nested2".into(), AttributeValue::String("{nested1}".into()))
            .is_ok()
    );

    let input = "Here is a +special {nested2} value+ to test.";
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;

    assert_eq!(
        result.text,
        "Here is a \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} to test."
    );

    assert_eq!(result.passthroughs.len(), 1);
    let Some(first) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert!(matches!(
        &first.text,
        Some(s) if *s == "special {nested2} value"
    ));

    let start_pos = first.location.absolute_start;
    let end_pos = first.location.absolute_end;
    assert_eq!(start_pos, 10); // Start of passthrough content
    assert_eq!(end_pos, 35); // End of passthrough content
    Ok(())
}

#[test]
fn test_line_breaks() -> Result<(), Error> {
    let attributes = setup_attributes();

    let input = "This is a test +\nwith a line break.";
    let state = setup_state(input);
    //                 012345678901234567890123456789012345678
    //                 0         1         2         3         4
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(result.text, "This is a test +\nwith a line break.");

    assert_eq!(result.passthroughs, []);
    Ok(())
}

#[test]
fn test_section_with_passthrough() -> Result<(), Error> {
    let attributes = setup_attributes();
    // Greedy matching: +<h1>+World+ matches (content: <h1>+World), +<u>+Gemini+ matches (content: <u>+Gemini)
    let input = "= Document Title\nHello +<h1>+World+</h1>+ of +<u>+Gemini+</u>+";
    //                 0123456789012345678901234567890123456789012345678901234567890
    //                 0         1         2         3         4         5         6
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;

    // Two passthroughs with greedy matching (not four)
    assert_eq!(
        result.text,
        "= Document Title\nHello \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}</h1>+ of \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD}</u>+"
    );

    assert_eq!(result.passthroughs.len(), 2);

    let Some(first_pass) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    let Some(second_pass) = result.passthroughs.get(1) else {
        panic!("expected second passthrough");
    };

    assert!(matches!(&first_pass.text, Some(s) if *s == "<h1>+World"));
    assert!(matches!(&second_pass.text, Some(s) if *s == "<u>+Gemini"));

    assert!(
        first_pass
            .substitutions
            .contains(&Substitution::SpecialChars)
    );
    assert!(
        second_pass
            .substitutions
            .contains(&Substitution::SpecialChars)
    );

    Ok(())
}

#[test]
fn test_pass_macro_with_mixed_content() -> Result<(), Error> {
    let mut attributes = setup_attributes();
    assert!(
        attributes
            .insert("docname".into(), AttributeValue::String("test-doc".into()))
            .is_ok()
    );

    let input = "The text pass:q,a[<u>underline _{docname}_</u>] is underlined.";
    let state = setup_state(input);
    //                 01234567890123456789012345678901234567890123456789012345678901
    //                 0         1         2         3         4         5         6
    //                          ^start of pass        ^docname
    //                "The text FFF0FFF is underlined."
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(
        result.text,
        "The text \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} is underlined."
    );

    assert_eq!(result.passthroughs.len(), 1);

    let Some(pass) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };

    assert!(matches!(
        &pass.text,
        Some(s) if *s == "<u>underline _{docname}_</u>"
    ));

    assert!(pass.substitutions.contains(&Substitution::Quotes)); // 'q'
    assert!(pass.substitutions.contains(&Substitution::Attributes)); // 'a'

    assert_eq!(pass.location.absolute_start, 9); // Start of pass macro
    assert_eq!(pass.location.absolute_end, 47); // End of pass macro content including brackets

    assert_eq!(result.source_map.map_position(9)?, 9); // Start of pass macro
    assert_eq!(result.source_map.map_position(28)?, 47); // First byte after the marker
    Ok(())
}

#[test]
fn test_all_passthroughs_with_attribute() -> Result<(), Error> {
    let mut attributes = setup_attributes();
    assert!(
        attributes
            .insert("meh".into(), AttributeValue::String("1.0".into()))
            .is_ok()
    );

    let input = "1 +2+, ++3++ {meh} and +++4+++ are all numbers.";
    //                 012345678901234567890123456789012345678901234567890123456789012345678901234567890123456
    //                 0         1         2         3         4         5         6         7         8
    //                 1 FFF0FFF, FFF1FFF 1.0 and FFF2FFF are all numbers.
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(
        result.text,
        "1 \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}, \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} 1.0 and \u{FFFD}\u{FFFD}\u{FFFD}2\u{FFFD}\u{FFFD}\u{FFFD} are all numbers."
    );

    assert_eq!(result.passthroughs.len(), 3);
    let Some(first) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    let Some(second) = result.passthroughs.get(1) else {
        panic!("expected second passthrough");
    };
    let Some(third) = result.passthroughs.get(2) else {
        panic!("expected third passthrough");
    };
    assert!(matches!(first.kind, PassthroughKind::Single));
    assert!(matches!(second.kind, PassthroughKind::Double));
    assert!(matches!(third.kind, PassthroughKind::Triple));
    assert!(matches!(&first.text, Some(s) if *s == "2"));
    assert!(matches!(&second.text, Some(s) if *s == "3"));
    assert!(matches!(&third.text, Some(s) if *s == "4"));

    assert_eq!(result.source_map.map_position(2)?, 2);
    // The placeholder position maps to the end of the original +2+ span.
    assert_eq!(result.source_map.map_position(5)?, 4);
    // 24 is within passthrough 2 and maps to its original content.
    assert_eq!(result.source_map.map_position(24)?, 8);
    // 48 is the n in "and".
    assert_eq!(result.source_map.map_position(48)?, 20);
    Ok(())
}

#[test]
fn test_greedy_matching_single_plus_passthrough() -> Result<(), Error> {
    let attributes = setup_attributes();
    // Test case 1: +A+B+ should greedily match from first to third +
    let input = "Test +A+B+ end";
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(result.passthroughs.len(), 1);
    let Some(first) = result.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert!(matches!(&first.text, Some(s) if *s == "A+B"));

    // Test case 2: +A+ +B+ should create two separate passthroughs (space breaks greedy)
    let input2 = "Test +A+ +B+ end";
    let state2 = setup_state(input2);
    let result2 = inline_preprocessing::run(input2, &attributes, &state2)?;
    assert_eq!(result2.passthroughs.len(), 2);
    let Some(first) = result2.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    let Some(second) = result2.passthroughs.get(1) else {
        panic!("expected second passthrough");
    };
    assert!(matches!(&first.text, Some(s) if *s == "A"));
    assert!(matches!(&second.text, Some(s) if *s == "B"));

    // Test case 3: +A+B+C+D+ should greedily match all
    let input3 = "Test +A+B+C+D+ end";
    let state3 = setup_state(input3);
    let result3 = inline_preprocessing::run(input3, &attributes, &state3)?;
    assert_eq!(result3.passthroughs.len(), 1);
    let Some(first) = result3.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert!(matches!(&first.text, Some(s) if *s == "A+B+C+D"));

    // Test case 4: +HTML+tags+ with boundary characters
    let input4 = "Test +<em>+text+ end";
    let state4 = setup_state(input4);
    let result4 = inline_preprocessing::run(input4, &attributes, &state4)?;
    assert_eq!(result4.passthroughs.len(), 1);
    let Some(first) = result4.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert!(matches!(&first.text, Some(s) if *s == "<em>+text"));

    // Test case 5: Multiple + with punctuation boundaries
    let input5 = "Look +here+there+, ok";
    let state5 = setup_state(input5);
    let result5 = inline_preprocessing::run(input5, &attributes, &state5)?;
    assert_eq!(result5.passthroughs.len(), 1);
    let Some(first) = result5.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    assert!(matches!(&first.text, Some(s) if *s == "here+there"));

    // A plus inside the content must not close the passthrough before its boundary.
    let input6 = "Hello +<h1>+World+</h1>+ and +<u>+Gemini+</u>+ end";
    let state6 = setup_state(input6);
    let result6 = inline_preprocessing::run(input6, &attributes, &state6)?;
    assert_eq!(result6.passthroughs.len(), 2);
    let Some(first) = result6.passthroughs.first() else {
        panic!("expected first passthrough");
    };
    let Some(second) = result6.passthroughs.get(1) else {
        panic!("expected second passthrough");
    };
    assert!(matches!(&first.text, Some(s) if *s == "<h1>+World"));
    assert!(matches!(&second.text, Some(s) if *s == "<u>+Gemini"));

    Ok(())
}

/// Comprehensive test for all character replacement attributes.
///
/// Tests all 31 attributes defined in the `AsciiDoc` specification:
/// <https://docs.asciidoctor.org/asciidoc/latest/attributes/character-replacement-ref/>
///
/// Note: `{lt}`, `{gt}`, `{amp}` are treated as passthroughs and produce placeholders
/// in the preprocessed text. They are resolved to `RawText` nodes during passthrough
/// processing, which bypasses HTML escaping.
#[test]
fn test_all_character_replacement_attributes() -> Result<(), Error> {
    let attributes = DocumentAttributes::default();
    let input = concat!(
        // Whitespace & invisible
        "{empty}{blank}{sp}{nbsp}{zwsp}{wj}",
        // Quotes
        "{apos}{quot}{lsquo}{rsquo}{ldquo}{rdquo}",
        // Symbols
        "{deg}{plus}{brvbar}{vbar}{amp}{lt}{gt}",
        // Syntax escaping
        "{startsb}{endsb}{caret}{asterisk}{tilde}{backslash}{backtick}",
        // Sequences
        "{two-colons}{two-semicolons}{cpp}{cxx}{pp}"
    );
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;

    // Build expected output by concatenating all expected values.
    // ASCII character replacement attributes produce passthrough placeholders
    // to prevent the PEG grammar from misinterpreting their values as AsciiDoc syntax.
    // Passthrough indices are assigned in order of appearance.
    let p = |i: usize| format!("\u{FFFD}\u{FFFD}\u{FFFD}{i}\u{FFFD}\u{FFFD}\u{FFFD}");
    let expected = format!(
        concat!(
            // Whitespace: empty, blank, space, nbsp, zwsp, wj (not passthroughs)
            "", "", " ", "\u{00A0}", "\u{200B}", "\u{2060}",
            // Quotes: apos(p0), quot(p1), lsquo, rsquo, ldquo, rdquo
            "{}", // apos
            "{}", // quot
            "\u{2018}", "\u{2019}", "\u{201C}", "\u{201D}",
            // Symbols: deg, plus(p2), brvbar, vbar(p3), amp(p4), lt(p5), gt(p6)
            "\u{00B0}", "{}", // plus
            "\u{00A6}", "{}", // vbar
            "{}", // amp
            "{}", // lt
            "{}", // gt
            // Escaping: startsb(p7), endsb(p8), caret(p9), asterisk(p10),
            //           tilde(p11), backslash(p12), backtick(p13)
            "{}", // startsb
            "{}", // endsb
            "{}", // caret
            "{}", // asterisk
            "{}", // tilde
            "{}", // backslash
            "{}", // backtick
            // Sequences: two-colons(p14), two-semicolons(p15), cpp(p16), cxx(p17), pp(p18)
            "{}", // two-colons
            "{}", // two-semicolons
            "{}", // cpp
            "{}", // cxx
            "{}", // pp
        ),
        p(0),
        p(1),
        p(2),
        p(3),
        p(4),
        p(5),
        p(6),
        p(7),
        p(8),
        p(9),
        p(10),
        p(11),
        p(12),
        p(13),
        p(14),
        p(15),
        p(16),
        p(17),
        p(18),
    );

    assert_eq!(
        result.text, expected,
        "Character replacement attributes did not produce expected values"
    );

    assert_eq!(
        result.passthroughs.len(),
        19,
        "Should have 19 passthroughs for all ASCII char replacement attributes"
    );
    // Spot-check a few key passthroughs
    assert_eq!(result.passthroughs[0].text, Some("&#39;")); // apos
    assert_eq!(result.passthroughs[2].text, Some("+")); // plus
    assert_eq!(result.passthroughs[4].text, Some("&")); // amp
    assert_eq!(result.passthroughs[16].text, Some("C++")); // cpp

    Ok(())
}

/// Test that character replacement attributes work in context.
#[test]
fn test_character_replacement_in_context() -> Result<(), Error> {
    let attributes = DocumentAttributes::default();

    // Test 1: Attributes in sentence
    let input1 = "The temperature is 100{deg}F";
    let state1 = setup_state(input1);
    let result1 = inline_preprocessing::run(input1, &attributes, &state1)?;
    assert_eq!(result1.text, "The temperature is 100\u{00B0}F");

    // Test 2: Multiple attributes (produce passthrough placeholders)
    let input2 = "Use {startsb}option{endsb} syntax";
    let state2 = setup_state(input2);
    let result2 = inline_preprocessing::run(input2, &attributes, &state2)?;
    assert_eq!(
        result2.text,
        "Use \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}option\u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} syntax"
    );
    assert_eq!(result2.passthroughs.len(), 2);
    assert_eq!(result2.passthroughs[0].text, Some("["));
    assert_eq!(result2.passthroughs[1].text, Some("]"));

    // Test 3: Adjacent attributes (Unicode chars, not passthroughs)
    let input3 = "{ldquo}Hello{rdquo}";
    let state3 = setup_state(input3);
    let result3 = inline_preprocessing::run(input3, &attributes, &state3)?;
    assert_eq!(result3.text, "\u{201C}Hello\u{201D}");

    // Test 4: Empty/blank produce no visible output
    let input4 = "before{empty}after";
    let state4 = setup_state(input4);
    let result4 = inline_preprocessing::run(input4, &attributes, &state4)?;
    assert_eq!(result4.text, "beforeafter");

    let input5 = "before{blank}after";
    let state5 = setup_state(input5);
    let result5 = inline_preprocessing::run(input5, &attributes, &state5)?;
    assert_eq!(result5.text, "beforeafter");

    // Test 5: C++ variations (produce passthrough placeholders)
    let input6 = "{cpp} is same as {cxx}";
    let state6 = setup_state(input6);
    let result6 = inline_preprocessing::run(input6, &attributes, &state6)?;
    assert_eq!(
        result6.text,
        "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} is same as \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD}"
    );
    assert_eq!(result6.passthroughs.len(), 2);
    assert_eq!(result6.passthroughs[0].text, Some("C++"));
    assert_eq!(result6.passthroughs[1].text, Some("C++"));

    Ok(())
}

#[test]
fn test_counter_reference_collects_warning() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "Count: {counter:mycount}";
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    // Counter is removed from output
    assert_eq!(result.text, "Count: ");
    // Warning is collected, not emitted directly
    let warnings = state.warnings.borrow();
    assert_eq!(warnings.len(), 1);
    let msg = warnings[0].to_string();
    assert!(msg.contains("counter"), "got: {msg}");
    assert!(msg.contains("mycount"), "got: {msg}");
    // Counter warning should carry a location pointing at the `{counter:..}` span.
    assert!(warnings[0].source_location().is_some());
    Ok(())
}

#[test]
fn test_counter_reference_deduplication_is_per_position() -> Result<(), Error> {
    let attributes = setup_attributes();
    // Same counter referenced at two different positions — each position is a
    // distinct diagnostic site, so LSP gets two separate warnings to squiggle.
    // Dedup only folds duplicates at the same position (PEG backtracking).
    let input = "{counter:hits} and {counter:hits}";
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(result.text, " and ");
    let warnings = state.warnings.borrow();
    assert_eq!(
        warnings.len(),
        2,
        "each counter reference is its own diagnostic site, got: {warnings:?}",
    );
    // Both warnings should carry distinct source locations.
    let loc0 = warnings[0].source_location().expect("loc0");
    let loc1 = warnings[1].source_location().expect("loc1");
    assert_ne!(loc0, loc1, "locations should differ between occurrences");
    Ok(())
}

#[test]
fn test_distinct_counter_references_produce_separate_warnings() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "{counter:a} and {counter2:b}";
    let state = setup_state(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(result.text, " and ");
    let warnings = state.warnings.borrow();
    assert_eq!(
        warnings.len(),
        2,
        "different counter warnings should both be collected"
    );
    Ok(())
}

fn setup_state_macros_disabled(content: &str) -> InlinePreprocessorParserState<'_> {
    let arena: &'static Bump = Box::leak(Box::new(Bump::new()));
    InlinePreprocessorParserState {
        pass_found_count: Cell::new(0),
        passthroughs: RefCell::new(Vec::new()),
        current_offset: Cell::new(0),
        line_map: Rc::new(LineMap::new(content)),
        full_input: content,
        arena,
        source_map: RefCell::new(SourceMap::default()),
        input: RefCell::new(content),
        substring_start_offset: Cell::new(0),
        warnings: RefCell::new(Vec::new()),
        macros_enabled: false,
        attributes_enabled: true,
        defer_monospace: true,
        attribute_value_ranges: Vec::new(),
    }
}

#[test]
fn test_pass_macro_a_with_macros_disabled_expands_attributes() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "pass:a[{version}]";
    let state = setup_state_macros_disabled(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(result.text, "pass:a[1.0]");
    assert_eq!(result.passthroughs, []);
    Ok(())
}

#[test]
fn test_pass_macro_no_subs_with_macros_disabled_expands_attributes() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "pass:[{version}]";
    let state = setup_state_macros_disabled(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(result.text, "pass:[1.0]");
    assert_eq!(result.passthroughs, []);
    Ok(())
}

#[test]
fn test_pass_macro_q_with_macros_disabled_preserves_content() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "pass:q[text]";
    let state = setup_state_macros_disabled(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(result.text, "pass:q[text]");
    assert_eq!(result.passthroughs, []);
    Ok(())
}

#[test]
fn test_pass_macro_a_q_with_macros_disabled_expands_attributes() -> Result<(), Error> {
    let attributes = setup_attributes();
    let input = "pass:a,q[{version}]";
    let state = setup_state_macros_disabled(input);
    let result = inline_preprocessing::run(input, &attributes, &state)?;
    assert_eq!(result.text, "pass:a,q[1.0]");
    assert_eq!(result.passthroughs, []);
    Ok(())
}
