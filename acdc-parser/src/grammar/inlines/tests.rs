use std::mem::discriminant;

use crate::{CompatibilityMode, Error, InlineMacro, InlineNode, Options, parse_inline};

#[test]
fn ordinary_runs_preserve_inline_constructs_and_locations() -> Result<(), Error> {
    // Cross the email lookahead window and switch between ASCII and Unicode
    // runs before each construct. Compare against the same construct alone.
    let prefix = "alpha beta gamma delta Élan 123; ".repeat(70);
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("experimental", "")
            .build()?;
        for syntax in [
            "anchor:here[]",
            "image:photo.png[Photo]",
            "icon:heart[]",
            "footnote:[A note]",
            "xref:there[There]",
            "link:page.html[Page]",
            "mailto:person@example.com[Mail]",
            "https://example.com[Site]",
            "http://example.com",
            "ftp://example.com/file",
            "irc://example.com/channel",
            "person@example.com",
            "\"Jane Doe\"@example.com",
            "_name@example.com",
            "%name@example.com",
            "+name@example.com",
            "<mailto:person@example.com>",
            "kbd:[Ctrl+C]",
            "btn:[Save]",
            "menu:File[Open]",
            "stem:[x]",
            "asciimath:[x]",
            "latexmath:[x]",
            "indexterm:[index]",
            "indexterm2:[index]",
            "*bold*",
            "`code`",
        ] {
            let standalone = parse_inline(syntax, &options)?;
            let input = format!("{prefix}{syntax} trailing text");
            let parsed = parse_inline(&input, &options)?;
            let is_construct = |node: &&InlineNode<'_>| !matches!(node, InlineNode::PlainText(_));
            let expected = standalone
                .inlines()
                .iter()
                .find(is_construct)
                .expect(syntax);
            let actual = parsed.inlines().iter().find(is_construct).expect(syntax);
            assert_eq!(
                discriminant(actual),
                discriminant(expected),
                "{syntax} ({mode:?})"
            );
            if let (InlineNode::Macro(actual), InlineNode::Macro(expected)) = (actual, expected) {
                assert_eq!(
                    discriminant(actual),
                    discriminant(expected),
                    "{syntax} ({mode:?})"
                );
            }
            assert_eq!(
                actual.location().absolute_start,
                prefix.len() + expected.location().absolute_start,
                "{syntax} ({mode:?})"
            );
            assert_eq!(
                actual.location().absolute_end,
                prefix.len() + expected.location().absolute_end,
                "{syntax} ({mode:?})"
            );
        }
    }
    Ok(())
}

#[test]
fn punctuation_email_starts_keep_existing_plain_text_boundaries() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for punctuation in [".", "-"] {
            for prefix in ["", "ordinary "] {
                let input = format!("{prefix}{punctuation}name@example.com");
                let parsed = parse_inline(&input, &options)?;
                let node = parsed.inlines().last().expect("an email node");
                assert!(matches!(node, InlineNode::Macro(InlineMacro::Autolink(_))));
                // In an existing plain-text run, punctuation belongs to that
                // run. At the start of input it can belong to the email instead.
                let start = if prefix.is_empty() {
                    0
                } else {
                    prefix.len() + 1
                };
                assert_eq!(node.location().absolute_start, start);
                assert_eq!(node.location().absolute_end, input.len() - 1);
            }
        }
    }
    Ok(())
}

#[test]
fn formatting_runs_preserve_nested_links_and_closing_boundaries() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for input in [
            "*Élan and 日本語 link:page.html[label *with* markers] tail*",
            "*World http://example.com[label *with* markers] tail*",
            "*World mailto:name@example.com[label *with* markers] tail*",
            "*World xref:there[label *with* markers] tail*",
            "*World <<there,label *with* markers>> tail*",
            "*invalid*closer then a valid closer*",
            "*a closing marker after whitespace * stays inside*",
            "*ordinary links: mailtos: xreferences: httpish: words*",
        ] {
            let parsed = parse_inline(input, &options)?;
            let [InlineNode::BoldText(bold)] = parsed.inlines() else {
                return Err(
                    std::io::Error::other(format!("expected one bold span: {input}")).into(),
                );
            };
            assert_eq!(bold.location.absolute_start, 0, "{input} ({mode:?})");
            assert_eq!(
                bold.location.absolute_end,
                input.len() - 1,
                "{input} ({mode:?})"
            );
        }
    }
    Ok(())
}
