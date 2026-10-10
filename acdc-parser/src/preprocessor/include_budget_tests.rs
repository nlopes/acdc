use std::sync::{Arc, Mutex, PoisonError};

use super::*;
use crate::{IncludeLoader, SafeMode};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Sources {
    files: Vec<(&'static str, &'static str)>,
    opened: Mutex<Vec<String>>,
}

impl Sources {
    fn new(files: &[(&'static str, &'static str)]) -> Arc<Self> {
        Arc::new(Self {
            files: files.to_vec(),
            opened: Mutex::default(),
        })
    }

    fn options(self: &Arc<Self>) -> Result<Options<'static>, Error> {
        let provider: Arc<Self> = Arc::clone(self);
        Options::builder()
            .with_safe_mode(SafeMode::Safe)
            .with_base_dir(std::env::temp_dir().join("acdc-include-budget"))
            .with_include_loader(IncludeLoader::Custom(provider))
            .with_attribute("allow-uri-read", true)
            .build()
    }

    fn opened(&self) -> Vec<String> {
        self.opened
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

fn process(input: &str, options: &Options<'_>, budget: usize) -> Result<String, Error> {
    let options = options
        .clone()
        .into_builder()
        .with_max_total_include_bytes(budget)
        .build()?;
    let preprocessor = Preprocessor::new(&options, Rc::default());
    let source = SourceOrigin::entry_file(
        &options
            .base_dir
            .as_ref()
            .ok_or_else(|| std::io::Error::other("missing test base directory"))?
            .join("entry.adoc"),
        options.base_dir.as_deref(),
    )?;
    preprocessor
        .process_inner(input, Some(&source), &mut options.clone())
        .map(|result| result.text.into_owned())
}

impl IncludeSourceProvider for Sources {
    fn open(&self, target: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        let name = match target {
            IncludeSourceTarget::File(path) => path.file_name().and_then(|name| name.to_str()),
            IncludeSourceTarget::Uri(uri) => uri.rsplit('/').next(),
        }
        .ok_or_else(|| {
            IncludeSourceError::new(IncludeSourceErrorKind::Fatal, "invalid test target")
        })?;
        self.opened
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(name.to_owned());
        self.files
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, content)| IncludeSource::from_bytes(content.as_bytes().to_vec()))
            .ok_or_else(|| IncludeSourceError::new(IncludeSourceErrorKind::NotFound, "missing"))
    }
}

fn assert_budget_error(result: Result<String, Error>, file: &str, line: u32) -> TestResult {
    let Err(error) = result else {
        return Err("expected an include budget error".into());
    };
    assert!(
        matches!(error, Error::IncludeExpansionTooLarge(..)),
        "{error}"
    );
    let source = error.source_location().ok_or("missing error location")?;
    assert_eq!(
        source
            .file
            .as_ref()
            .and_then(|path| path.file_name())
            .ok_or("missing error file")?,
        file
    );
    assert_eq!(source.location.start.line, line);
    assert_eq!(source.location.start.column, 1);
    assert!(
        error
            .advice()
            .ok_or("missing error advice")?
            .contains("Select less include text")
    );
    Ok(())
}

#[test]
fn include_budget_is_shared_by_siblings_and_counts_utf8_bytes() -> TestResult {
    let sources = Sources::new(&[("a.log", "é"), ("b.log", "bbb")]);
    let options = sources.options()?;
    let input = "include::a.log[]\ninclude::b.log[]";
    assert_eq!(process(input, &options, 7)?, "é\nbbb");
    assert_budget_error(process(input, &options, 6), "entry.adoc", 2)?;
    Ok(())
}

#[test]
fn include_budget_counts_repeated_targets_and_stops_on_failure() -> TestResult {
    let sources = Sources::new(&[("a.log", "abc"), ("later.log", "unused")]);
    let options = sources.options()?;
    let error = process(
        "include::a.log[]\ninclude::a.log[]\ninclude::later.log[]",
        &options,
        7,
    );
    assert_budget_error(error, "entry.adoc", 2)?;
    assert_eq!(sources.opened(), ["a.log", "a.log"]);
    Ok(())
}

#[test]
fn include_budget_is_shared_by_nested_and_sibling_includes() -> TestResult {
    let nested = "include::leaf.log[]";
    let sources = Sources::new(&[("chapter.adoc", nested), ("leaf.log", "abc")]);
    let options = sources.options()?;
    let input = "include::chapter.adoc[]\ninclude::leaf.log[]";
    let budget = nested.len() + 1 + 4 + 4;
    assert_eq!(process(input, &options, budget)?, "abc\nabc");
    assert_budget_error(process(input, &options, budget - 1), "entry.adoc", 2)?;
    Ok(())
}

#[test]
fn include_budget_error_retains_the_selected_source_line() -> TestResult {
    let sources = Sources::new(&[
        ("chapter.adoc", "skip\nskip\ninclude::leaf.log[]"),
        ("leaf.log", "abc"),
    ]);
    let options = sources.options()?;
    let error = process(
        "include::chapter.adoc[lines=3]",
        &options,
        "include::leaf.log[]".len() + 1,
    );
    assert_budget_error(error, "chapter.adoc", 3)?;
    Ok(())
}

#[test]
fn include_budget_checks_parent_text_before_opening_nested_targets() -> TestResult {
    let sources = Sources::new(&[("chapter.adoc", "include::leaf.log[]"), ("leaf.log", "abc")]);
    let options = sources.options()?;
    let error = process("include::chapter.adoc[]", &options, 1);
    assert_budget_error(error, "entry.adoc", 1)?;
    assert_eq!(sources.opened(), ["chapter.adoc"]);
    Ok(())
}

#[test]
fn include_budget_counts_selection_after_indentation() -> TestResult {
    let sources = Sources::new(&[("a.log", "ignored\nx\nignored")]);
    let options = sources.options()?;
    let input = "include::a.log[lines=2,indent=3]";
    assert_eq!(process(input, &options, 5)?, "   x");
    assert_budget_error(process(input, &options, 4), "entry.adoc", 1)?;
    Ok(())
}

#[test]
fn include_budget_is_shared_by_local_and_custom_uri_sources() -> TestResult {
    let sources = Sources::new(&[("a.log", "abc")]);
    let options = sources.options()?;
    let error = process(
        "include::a.log[]\ninclude::https://example.test/a.log[]",
        &options,
        7,
    );
    assert_budget_error(error, "entry.adoc", 2)?;
    Ok(())
}

#[test]
fn include_budget_does_not_count_entry_text_or_unread_includes() -> TestResult {
    let sources = Sources::new(&[("a.log", "abc"), ("empty.log", "")]);
    let options = sources.options()?;
    let input = "Entry text\nifdef::absent[]\ninclude::a.log[]\nendif::[]\ninclude::missing.log[opts=optional]\ninclude::empty.log[]";
    assert_eq!(process(input, &options, 0)?, "Entry text");
    assert_eq!(sources.opened(), ["missing.log", "empty.log"]);
    for options in [
        Options {
            safe_mode: SafeMode::Secure,
            ..options.clone()
        },
        Options {
            include_loader: IncludeLoader::Disabled,
            ..options.clone()
        },
    ] {
        process("include::a.log[]", &options, 0)?;
    }
    assert_eq!(sources.opened(), ["missing.log", "empty.log"]);
    Ok(())
}

#[test]
fn include_budget_counts_text_before_comments_and_conditionals_are_removed() -> TestResult {
    for content in ["// comment", "ifdef::absent[]\nignored\nendif::[]"] {
        let sources = Sources::new(&[("chapter.adoc", content)]);
        let options = sources.options()?;
        let input = "include::chapter.adoc[]";
        process(input, &options, content.len() + 1)?;
        assert_budget_error(process(input, &options, content.len()), "entry.adoc", 1)?;
    }
    Ok(())
}

#[test]
fn include_budget_resets_between_parses_with_the_same_options() -> TestResult {
    let sources = Sources::new(&[("a.log", "abc")]);
    let options = sources.options()?;
    assert!(process("include::a.log[]\ninclude::a.log[]", &options, 4).is_err());
    assert_eq!(
        Preprocessor::process("include::a.log[]", &options, Rc::default())?.text,
        "abc"
    );
    assert_eq!(
        Preprocessor::process_reader("include::a.log[]".as_bytes(), &options, Rc::default())?.text,
        "abc"
    );
    Ok(())
}
