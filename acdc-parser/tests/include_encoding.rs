use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{Block, Error as ParserError, InlineNode, Options, ParseResult, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> std::io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acdc-parser-include-encoding-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, content: impl AsRef<[u8]>) -> std::io::Result<PathBuf> {
        let path = self.0.join(relative);
        fs::write(&path, content)?;
        Ok(path)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn paragraph_texts(result: &ParseResult) -> Result<Vec<String>, Box<dyn Error>> {
    result
        .document()
        .blocks
        .iter()
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return Err(format!("expected paragraph, got {block:?}").into());
            };
            paragraph
                .content
                .iter()
                .map(|node| {
                    let InlineNode::PlainText(text) = node else {
                        return Err(format!("expected plain text, got {node:?}").into());
                    };
                    Ok(text.content)
                })
                .collect()
        })
        .collect()
}

fn utf16_bytes(content: &str, little_endian: bool, bom: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    if bom {
        bytes.extend(if little_endian {
            [0xFF, 0xFE]
        } else {
            [0xFE, 0xFF]
        });
    }
    for code_unit in content.encode_utf16() {
        bytes.extend(if little_endian {
            code_unit.to_le_bytes()
        } else {
            code_unit.to_be_bytes()
        });
    }
    bytes
}

fn parse_include(directory: &TempDirectory, attributes: &str) -> Result<ParseResult, ParserError> {
    let main = directory
        .write(
            "main.adoc",
            format!("BEFORE\n\ninclude::part.adoc[{attributes}]\n\nAFTER"),
        )
        .map_err(ParserError::from)?;
    parse_file(main, &Options::default())
}

#[test]
fn utf16_bom_and_explicit_endian_labels_decode_to_utf8() -> TestResult {
    for (little_endian, attributes) in [
        (true, ""),
        (false, ""),
        (true, "encoding=UTF-16LE"),
        (false, "encoding=UTF-16BE"),
    ] {
        let directory = TempDirectory::new()?;
        directory.write(
            "part.adoc",
            utf16_bytes("Café", little_endian, attributes.is_empty()),
        )?;

        let result = parse_include(&directory, attributes)?;

        assert_eq!(paragraph_texts(&result)?, ["BEFORE", "Café", "AFTER"]);
        assert!(result.warnings().is_empty());
    }
    Ok(())
}

#[test]
fn representative_single_byte_encodings_and_aliases_decode_to_utf8() -> TestResult {
    for encoding in ["Windows-1252", "CP1252"] {
        let directory = TempDirectory::new()?;
        directory.write("part.adoc", b"Caf\xe9 \x80")?;

        let result = parse_include(&directory, &format!("encoding={encoding}"))?;

        assert_eq!(paragraph_texts(&result)?, ["BEFORE", "Café €", "AFTER"]);
        assert!(result.warnings().is_empty());
    }
    Ok(())
}

#[test]
fn unknown_encoding_label_is_ignored_before_utf8_fallback() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("part.adoc", "Café")?;

    let result = parse_include(&directory, "encoding=not-a-ruby-encoding")?;

    assert_eq!(paragraph_texts(&result)?, ["BEFORE", "Café", "AFTER"]);
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn malformed_explicit_utf16_recovers_as_an_unreadable_local_include() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("part.adoc", [0x00, 0xD8])?;

    let result = parse_include(&directory, "encoding=UTF-16LE")?;

    let texts = paragraph_texts(&result)?;
    assert_eq!(texts.first().map(String::as_str), Some("BEFORE"));
    assert!(
        texts
            .get(1)
            .is_some_and(|text| text.contains("include::part.adoc[encoding=UTF-16LE]"))
    );
    assert_eq!(texts.last().map(String::as_str), Some("AFTER"));
    let [warning] = result.warnings() else {
        return Err(format!("unexpected warnings: {:?}", result.warnings()).into());
    };
    assert!(
        warning
            .kind
            .to_string()
            .contains("include file not readable")
    );
    Ok(())
}

#[test]
fn malformed_utf8_without_transcoding_remains_fatal() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("part.adoc", [0xFF])?;

    let Err(error) = parse_include(&directory, "") else {
        return Err("invalid UTF-8 unexpectedly parsed".into());
    };

    assert!(matches!(error, ParserError::UnrecognizedEncodingInFile(_)));
    Ok(())
}

#[test]
fn selection_runs_after_decoding_the_complete_utf16_target() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write(
        "part.adoc",
        utf16_bytes("// tag::pick[]\nPICKED\n// end::pick[]", true, true),
    )?;

    let result = parse_include(&directory, "tag=pick")?;

    assert_eq!(paragraph_texts(&result)?, ["BEFORE", "PICKED", "AFTER"]);
    assert!(result.warnings().is_empty());
    Ok(())
}

#[test]
fn generic_utf16_without_a_bom_recovers_as_unreadable() -> TestResult {
    let directory = TempDirectory::new()?;
    directory.write("part.adoc", utf16_bytes("Café", true, false))?;

    let result = parse_include(&directory, "encoding=UTF-16")?;

    assert_eq!(result.warnings().len(), 1);
    assert!(
        paragraph_texts(&result)?
            .iter()
            .any(|text| text.contains("include::part.adoc[encoding=UTF-16]"))
    );
    Ok(())
}
