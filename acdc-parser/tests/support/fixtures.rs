use std::{
    error::Error as StdError,
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

use acdc_parser::{CompatibilityMode, Options, SafeMode, Warning, parse_file};
use serde_json::{Value, json};

pub type Error = Box<dyn StdError + Send + Sync>;

/// Selects an expectation filename, not a production parser policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Shared,
    Asciidoctor,
    Draft,
}

impl Variant {
    pub fn modes(self) -> &'static [CompatibilityMode] {
        match self {
            Self::Shared => &[CompatibilityMode::Asciidoctor, CompatibilityMode::Draft],
            Self::Asciidoctor => &[CompatibilityMode::Asciidoctor],
            Self::Draft => &[CompatibilityMode::Draft],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Asciidoctor => "asciidoctor",
            Self::Draft => "draft",
        }
    }

    fn path(self, source: &Path, extension: &str) -> PathBuf {
        match self {
            Self::Shared => source.with_extension(extension),
            Self::Asciidoctor | Self::Draft => {
                source.with_extension(format!("{}.{extension}", self.name()))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    Json,
    Error,
}

#[derive(Clone, Debug)]
pub struct Expectation {
    pub variant: Variant,
    kind: OutcomeKind,
    output: PathBuf,
    pub diagnostics: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct Case {
    root: PathBuf,
    source: PathBuf,
    pub name: String,
    pub expectations: Vec<Expectation>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Observation {
    kind: OutcomeKind,
    output: String,
    diagnostics: String,
}

pub fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests")
}

pub fn discover_sources(root: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut sources = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if entry.file_name() != "includes" {
                sources.extend(discover_sources(&path)?);
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension == "adoc")
        {
            sources.push(path);
        }
    }
    sources.sort();
    Ok(sources)
}

impl Case {
    pub fn load(root: &Path, source: &Path, allow_new: bool) -> Result<Self, Error> {
        let name = source.strip_prefix(root)?.with_extension("");
        let name = name
            .to_str()
            .ok_or("fixture names must be UTF-8")?
            .replace('\\', "/");
        let mut expectations = Vec::new();
        for variant in [Variant::Shared, Variant::Asciidoctor, Variant::Draft] {
            let json = variant.path(source, "json");
            let error = variant.path(source, "error");
            let diagnostics = variant.path(source, "diagnostics.json");
            let output = match (json.is_file(), error.is_file()) {
                (true, true) => {
                    return Err(format!(
                        "{name}: both JSON and error expectations for {}",
                        variant.name()
                    )
                    .into());
                }
                (true, false) => Some((OutcomeKind::Json, json)),
                (false, true) => Some((OutcomeKind::Error, error)),
                (false, false) => None,
            };
            if let Some((kind, output)) = output {
                expectations.push(Expectation {
                    variant,
                    kind,
                    output,
                    diagnostics: diagnostics.is_file().then_some(diagnostics),
                });
            } else if diagnostics.exists() {
                return Err(format!(
                    "{name}: diagnostics have no {} outcome expectation",
                    variant.name()
                )
                .into());
            }
        }
        match expectations.as_slice() {
            [] if allow_new => {
                expectations.push(Expectation {
                    variant: Variant::Shared,
                    kind: OutcomeKind::Json,
                    output: source.with_extension("json"),
                    diagnostics: None,
                });
            }
            [
                Expectation {
                    variant: Variant::Shared,
                    ..
                },
            ] => {}
            [
                asciidoctor @ Expectation {
                    variant: Variant::Asciidoctor,
                    ..
                },
                draft @ Expectation {
                    variant: Variant::Draft,
                    ..
                },
            ] => {
                if asciidoctor.diagnostics.is_some() != draft.diagnostics.is_some() {
                    return Err(format!("{name}: paired diagnostics require both modes").into());
                }
            }
            _ => {
                return Err(format!("{name}: require one shared expectation or a complete Asciidoctor/Draft pair, without mixing them").into());
            }
        }
        Ok(Self {
            root: root.to_path_buf(),
            source: source.to_path_buf(),
            name,
            expectations,
        })
    }

    pub fn test_name(&self, expectation: &Expectation) -> String {
        format!("{}::{}", self.name, expectation.variant.name())
    }

    pub fn unavailable_feature(&self) -> Option<&'static str> {
        let stem = self.source.file_stem()?.to_str()?;
        if !cfg!(feature = "setext") && stem.contains("with_setext") {
            Some("setext")
        } else {
            None
        }
    }

    fn options(&self, mode: CompatibilityMode) -> Result<Options<'static>, Error> {
        if let Some(feature) = self.unavailable_feature() {
            return Err(format!("{}: requires the {feature} feature", self.name).into());
        }
        let builder = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe);
        #[cfg(feature = "setext")]
        let builder = if self
            .source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem.contains("with_setext"))
        {
            builder.with_setext()
        } else {
            builder
        };
        Ok(builder.build()?)
    }

    pub fn observe(&self, mode: CompatibilityMode) -> Result<Observation, Error> {
        let options = self.options(mode)?;
        match parse_file(&self.source, &options) {
            Ok(parsed) => Ok(Observation {
                kind: OutcomeKind::Json,
                output: serde_json::to_string_pretty(parsed.document())?,
                diagnostics: serde_json::to_string_pretty(&json!({
                    "warnings": parsed.warnings().iter().map(|warning| self.warning(warning)).collect::<Vec<_>>(),
                    "recovery": parsed.source_recovery().map(|warning| self.warning(warning)),
                }))?,
            }),
            Err(error) => Ok(Observation {
                kind: OutcomeKind::Error,
                output: self.stable_text(&error.to_string()),
                diagnostics: serde_json::to_string_pretty(&Value::Null)?,
            }),
        }
    }

    fn stable_text(&self, text: &str) -> String {
        let text = text.replace(self.root.to_string_lossy().as_ref(), "<fixtures>");
        match self.root.canonicalize() {
            Ok(root) => text.replace(root.to_string_lossy().as_ref(), "<fixtures>"),
            Err(_) => text,
        }
    }

    fn warning(&self, warning: &Warning) -> Value {
        json!({
            "message": self.stable_text(&warning.kind.to_string()),
            "location": warning.location.as_ref().map(|location| {
                let file = location.file.as_ref().map(|path| {
                    let path = path.canonicalize().unwrap_or_else(|_| path.clone());
                    let root = self.root.canonicalize().unwrap_or_else(|_| self.root.clone());
                    path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/")
                });
                json!({ "file": file, "location": location.location, "absolute_start": location.location.absolute_start, "absolute_end": location.location.absolute_end })
            }),
        })
    }

    pub fn verify(&self, expected: &Expectation) -> Result<(), Error> {
        for mode in expected.variant.modes() {
            self.verify_mode(expected, *mode)?;
        }
        Ok(())
    }

    pub fn verify_mode(
        &self,
        expected: &Expectation,
        mode: CompatibilityMode,
    ) -> Result<(), Error> {
        let observed = self.observe(mode)?;
        self.check_outcome(expected, &observed)?;
        let content = fs::read_to_string(&expected.output)?;
        let content = if expected.kind == OutcomeKind::Error {
            content.trim()
        } else {
            &content
        };
        pretty_assertions::assert_eq!(
            content,
            observed.output,
            "{} ({mode:?})",
            self.test_name(expected)
        );
        if let Some(path) = &expected.diagnostics {
            let content = fs::read_to_string(path)?;
            pretty_assertions::assert_eq!(
                content,
                observed.diagnostics,
                "{} ({mode:?}) diagnostics",
                self.test_name(expected),
            );
        }
        Ok(())
    }

    fn check_outcome(&self, expected: &Expectation, observed: &Observation) -> Result<(), Error> {
        if expected.kind != observed.kind {
            return Err(format!(
                "{}: expected {:?}, got {:?}: {}",
                self.test_name(expected),
                expected.kind,
                observed.kind,
                observed.output
            )
            .into());
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct Generation {
    requested: Vec<PathBuf>,
    variant: Option<Variant>,
    pair: bool,
    diagnostics: bool,
}

impl Generation {
    pub fn from_args(args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<Self, Error> {
        let mut args = args.into_iter().map(Into::into);
        let mut request = Self::default();
        while let Some(arg) = args.next() {
            if arg == "--diagnostics" {
                request.diagnostics = true;
            } else if arg == "--pair" {
                if request.pair || request.variant.is_some() {
                    return Err("select --pair or one --mode, not both".into());
                }
                request.pair = true;
            } else if arg == "--mode" {
                if request.variant.is_some() || request.pair {
                    return Err("select --pair or one --mode, not both".into());
                }
                request.variant = Some(
                    match args.next().as_deref().and_then(|value| value.to_str()) {
                        Some("shared") => Variant::Shared,
                        Some("asciidoctor") => Variant::Asciidoctor,
                        Some("draft") => Variant::Draft,
                        _ => return Err("--mode requires shared, asciidoctor, or draft".into()),
                    },
                );
            } else {
                let path = PathBuf::from(arg);
                if path.extension().is_none_or(|extension| extension != "adoc")
                    || path
                        .components()
                        .any(|part| !matches!(part, Component::Normal(_)))
                {
                    return Err(format!(
                        "expected an exact relative .adoc filename, got {}",
                        path.display()
                    )
                    .into());
                }
                if !request.requested.contains(&path) {
                    request.requested.push(path);
                }
            }
        }
        if request.pair && request.requested.is_empty() {
            return Err("--pair requires exact fixture filenames".into());
        }
        Ok(request)
    }

    pub fn generate(&self, root: &Path) -> Result<usize, Error> {
        let sources = discover_sources(root)?;
        for requested in &self.requested {
            if !sources.contains(&root.join(requested)) {
                return Err(format!(
                    "fixture does not exist or is an include input: {}",
                    requested.display()
                )
                .into());
            }
        }
        let mut writes = Vec::new();
        let mut removals = Vec::new();
        let mut cases = 0;
        for source in sources {
            if !self.requested.is_empty()
                && !self.requested.iter().any(|path| root.join(path) == source)
            {
                continue;
            }
            let case = Case::load(root, &source, true)?;
            if let Some(feature) = case.unavailable_feature() {
                if !self.requested.is_empty() {
                    return Err(format!("{}: requires the {feature} feature", case.name).into());
                }
                eprintln!("Skipping {}: requires the {feature} feature", case.name);
                continue;
            }
            let variant = self.variant.unwrap_or(Variant::Shared);
            let variants = if self.pair {
                &[Variant::Asciidoctor, Variant::Draft]
            } else {
                std::slice::from_ref(&variant)
            };
            let shared = case
                .expectations
                .iter()
                .find(|expected| expected.variant == Variant::Shared);
            for variant in variants {
                let expected = case.expectations.iter()
                    .find(|expected| expected.variant == *variant)
                    .or(if self.pair { shared } else { None })
                    .ok_or_else(|| format!("{}: select an existing expectation with --mode, or use --pair to generate both modes", case.name))?;
                let mut observations = variant.modes().iter().map(|mode| case.observe(*mode));
                let observed = observations.next().ok_or("missing execution mode")??;
                for other in observations {
                    if observed != other? {
                        return Err(format!(
                            "{}: modes differ; use paired expectations",
                            case.name
                        )
                        .into());
                    }
                }
                case.check_outcome(expected, &observed)?;
                if *variant != Variant::Shared
                    && !self.pair
                    && self.diagnostics
                    && expected.diagnostics.is_none()
                {
                    return Err(format!(
                        "{}: use --pair to add diagnostics for both modes",
                        case.name
                    )
                    .into());
                }
                let extension = match expected.kind {
                    OutcomeKind::Json => "json",
                    OutcomeKind::Error => "error",
                };
                writes.push((variant.path(&source, extension), observed.output));
                if self.diagnostics || expected.diagnostics.is_some() {
                    writes.push((
                        variant.path(&source, "diagnostics.json"),
                        observed.diagnostics,
                    ));
                }
            }
            if self.pair
                && let Some(shared) = shared
            {
                if shared.output.is_file() {
                    removals.push(shared.output.clone());
                }
                removals.extend(shared.diagnostics.clone());
            }
            cases += 1;
        }
        // Parse every selected case before replacing any expectations.
        for (path, content) in writes {
            fs::write(&path, content)?;
            println!("Generated {}", path.display());
        }
        for path in removals {
            fs::remove_file(path)?;
        }
        Ok(cases)
    }
}
