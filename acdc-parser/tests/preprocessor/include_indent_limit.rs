use std::{
    error::Error,
    fs, io,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{Error as ParserError, Options, Position, parse_file};

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
fn excessive_include_indent_is_rejected_before_target_io() -> TestResult {
    let document = IncludeDocument::new("include::missing.adoc[indent=4097]")?;

    let Err(error) = parse_file(&document.main, &Options::default()) else {
        return Err("expected an excessive include indent to fail parsing".into());
    };
    let ParserError::IncludeIndentTooLarge(location, indent, limit) = error else {
        return Err(format!("unexpected parse error: {error:?}").into());
    };
    assert_eq!(indent, 4097);
    assert_eq!(limit, 4096);
    assert_eq!(location.file.as_deref(), Some(document.main.as_path()));
    assert_eq!(location.location.start, Position::new(1, 1));
    Ok(())
}
