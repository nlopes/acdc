use std::{
    error::Error,
    fs, io,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{CompatibilityMode, Options, Position, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct IncludeDocument {
    directory: PathBuf,
    main: PathBuf,
}

impl IncludeDocument {
    fn new(content: &str) -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "acdc-parser-include-indent-limit-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let main = directory.join("main.adoc");
        fs::write(&main, content)?;
        Ok(Self { directory, main })
    }
}

impl Drop for IncludeDocument {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn excessive_include_indent_is_ignored_without_losing_content() -> TestResult {
    let document = IncludeDocument::new("include::part.adoc[indent=4097]")?;
    fs::write(document.directory.join("part.adoc"), "  Original content.")?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &document.main,
            &Options::builder()
                .with_safe_mode(acdc_parser::SafeMode::Unsafe)
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        assert_eq!(parsed.preprocessed_source(), "  Original content.");
        let [warning] = parsed.warnings() else {
            return Err("expected one indentation warning".into());
        };
        let location = warning
            .location
            .as_ref()
            .ok_or("expected located warning")?;
        assert_eq!(location.file.as_deref(), Some(document.main.as_path()));
        assert_eq!(location.location.start, Position::new(1, 1));
        assert!(parsed.source_recovery().is_some());
    }
    Ok(())
}
