use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
    path::Path,
    process::{Command, Stdio},
};

use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_converters_manpage::Processor;
use acdc_parser::{Options as ParserOptions, parse};

type Error = Box<dyn std::error::Error>;

// Fixture words encode their intended font. Checking the rendered glyphs catches
// previous-font leaks that a syntactically valid roff snapshot cannot detect.
fn marker_font(word: &str) -> Option<u8> {
    let marker = word.strip_prefix('F').unwrap_or(word).as_bytes();
    match marker {
        [font @ (b'B' | b'I' | b'M' | b'R'), first, second]
            if first.is_ascii_digit() && second.is_ascii_digit() =>
        {
            Some(*font)
        }
        _ => None,
    }
}

fn render(program: &str, args: &[&str], input: &[u8]) -> Result<String, Error> {
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            io::Error::other(format!(
                "install {program} to run rendered manpage tests: {error}"
            ))
        })?;
    let write_result = child.stdin.take().map_or_else(
        || Err(io::Error::other("missing stdin")),
        |mut stdin| stdin.write_all(input),
    );
    let output = child.wait_with_output()?;
    write_result?;
    assert!(
        output.status.success(),
        "{program}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

fn check_groff(output: &str, expected: &HashSet<&str>) -> Result<(), Error> {
    let mut fonts = HashMap::new();
    let mut active = "";
    let mut seen = HashSet::new();
    for line in output.lines() {
        // groff may combine the word-space command with the next command.
        let line = line.strip_prefix('w').unwrap_or(line);
        if let Some(definition) = line.strip_prefix("x font ") {
            let mut fields = definition.split_whitespace();
            fonts.insert(
                fields.next().ok_or("missing font number")?,
                fields.next().ok_or("missing font name")?,
            );
        } else if let Some(number) = line.strip_prefix('f') {
            active = fonts.get(number).ok_or("undefined font")?;
        } else if let Some(text) = line.strip_prefix('t') {
            for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
                if let Some(font) = marker_font(word) {
                    let correct = match font {
                        b'B' => active.ends_with('B'),
                        b'I' => active.ends_with('I'),
                        b'M' => active == "CR",
                        _ => active == "TR",
                    };
                    assert!(correct, "groff: {word} rendered in {active}");
                    seen.insert(word);
                }
            }
        }
    }
    assert!(
        expected.is_subset(&seen),
        "groff lost markers: {:?}",
        expected.difference(&seen)
    );
    Ok(())
}

fn check_mandoc(output: &str, expected: &HashSet<&str>) -> Result<(), Error> {
    let mut text = Vec::new();
    let mut styles = Vec::new();
    let mut bytes = output.bytes().peekable();
    while let Some(mut glyph) = bytes.next() {
        let mut font = b'R';
        while bytes.peek() == Some(&b'\x08') {
            bytes.next();
            let next = bytes.next().ok_or("incomplete overstrike")?;
            font = if glyph == b'_' { b'I' } else { b'B' };
            glyph = next;
        }
        text.push(glyph);
        styles.push(font);
    }
    let mut seen = HashSet::new();
    let mut offset = 0;
    let text = String::from_utf8(text)?;
    for part in text.split_inclusive(|c: char| !c.is_ascii_alphanumeric()) {
        let word = part.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
        if let Some(font) = marker_font(word) {
            // The terminal device cannot distinguish roman from monospace.
            let font = if font == b'M' { b'R' } else { font };
            let actual = styles
                .get(offset..offset + word.len())
                .ok_or("missing glyph fonts")?;
            assert!(
                actual.iter().all(|&actual| actual == font),
                "mandoc: {word} has fonts {actual:?}",
            );
            seen.insert(word);
        }
        offset += part.len();
    }
    assert!(
        expected.is_subset(&seen),
        "mandoc lost markers: {:?}",
        expected.difference(&seen)
    );
    Ok(())
}

#[rstest::rstest]
#[case("nested_font_scopes")]
#[cfg_attr(feature = "pre-spec-subs", case("subs_nested_font_scopes"))]
fn nested_fonts_restore_the_enclosing_style(#[case] fixture: &str) -> Result<(), Error> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/source/{fixture}.adoc"));
    let source = std::fs::read_to_string(path)?;
    let expected: HashSet<_> = source
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| marker_font(word).is_some())
        .collect();
    let parsed = parse(&source, &ParserOptions::default())?;
    let processor = Processor::new(Options::default(), ParserOptions::builder())?;
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let warning_source = WarningSource::new("manpage");
    let mut diagnostics = Diagnostics::new(&warning_source, &mut warnings);
    processor.write_to(parsed.document(), &mut output, None, None, &mut diagnostics)?;
    check_groff(
        &render("groff", &["-Kutf8", "-Z", "-Tps", "-t", "-man"], &output)?,
        &expected,
    )?;
    check_mandoc(
        &render("mandoc", &["-Tascii", "-O", "width=120"], &output)?,
        &expected,
    )?;
    Ok(())
}
