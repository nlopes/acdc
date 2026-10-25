use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use acdc_parser::{
    AttributeValue, Block, IncludeLoader, IncludeSource, IncludeSourceError,
    IncludeSourceErrorKind, IncludeSourceTarget, Location, Options, ParseResult, SafeMode,
};
use nucleo_matcher::{
    Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::{
    config::{Config, is_asciidoc},
    error::{Error, Result},
    render::{self, Format},
};

const MAX_ASSEMBLED_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
struct Sources {
    total_bytes: usize,
    hashes: BTreeMap<PathBuf, Vec<u8>>,
}

pub(super) struct Document {
    path: PathBuf,
    parsed: ParseResult,
    revision: Vec<u8>,
}

impl Document {
    pub(super) fn load(
        config: &Config,
        input: &Path,
        attributes: &BTreeMap<String, AttributeValue<'static>>,
        strict: bool,
        cancelled: &CancellationToken,
        timestamp: u64,
    ) -> Result<Self> {
        if cancelled.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let path = config.resolve(input)?;
        if !is_asciidoc(&path) {
            return Err(Error::Input(
                "expected an .adoc, .asciidoc, or .asc file".into(),
            ));
        }
        let (_, raw) = config.read(&path)?;
        let text = std::str::from_utf8(&raw)
            .map_err(|_| Error::Input("entry documents must be UTF-8".into()))?;
        let sources = Arc::new(Mutex::new(Sources {
            hashes: BTreeMap::from([(path.clone(), Sha256::digest(&raw).to_vec())]),
            ..Sources::default()
        }));
        let loader = include_loader(config.clone(), sources.clone(), cancelled.clone());
        let mut options = Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_base_dir(path.parent().expect("absolute file has a parent"))
            .with_include_loader(loader)
            .with_max_total_include_bytes(MAX_ASSEMBLED_BYTES);
        #[cfg(feature = "setext")]
        {
            options = options.with_setext();
        }
        let timestamp = i64::try_from(timestamp)
            .ok()
            .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
            .ok_or_else(|| Error::Input("invalid processing timestamp".into()))?;
        let modified: chrono::DateTime<chrono::Utc> = std::fs::metadata(&path)?.modified()?.into();
        for (prefix, time) in [("local", timestamp), ("doc", modified)] {
            for (name, pattern) in [
                ("date", "%Y-%m-%d"),
                ("time", "%H:%M:%S UTC"),
                ("datetime", "%Y-%m-%d %H:%M:%S UTC"),
                ("year", "%Y"),
            ] {
                options = options.with_default_attribute(
                    format!("{prefix}{name}"),
                    time.format(pattern).to_string(),
                );
            }
        }
        options = options
            .with_attribute("docfile", path.to_string_lossy().into_owned())
            .with_attribute(
                "docdir",
                path.parent()
                    .expect("file parent")
                    .to_string_lossy()
                    .into_owned(),
            )
            .with_attribute(
                "docname",
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            )
            .with_attribute(
                "docfilesuffix",
                format!(
                    ".{}",
                    path.extension().unwrap_or_default().to_string_lossy()
                ),
            );
        for (name, value) in config.attributes().iter().chain(attributes) {
            options = options.with_attribute(name.clone(), value.clone());
        }
        let options = options.with_attribute("allow-uri-read", false).build()?;
        let parsed = acdc_parser::parse(text, &options)?;
        if cancelled.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if parsed.source().len() > MAX_ASSEMBLED_BYTES {
            return Err(Error::Input("assembled source exceeds 16 MiB".into()));
        }
        if strict && let Some(warning) = parsed.source_recovery() {
            return Err(Error::Input(format!(
                "strict processing rejected incomplete content: {warning}"
            )));
        }
        let mut hash = Sha256::new();
        for (path, digest) in &sources.lock().expect("source lock").hashes {
            hash.update(path.as_os_str().as_encoded_bytes());
            hash.update(digest);
        }
        Ok(Self {
            path,
            parsed,
            revision: hash.finalize().to_vec(),
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn parsed(&self) -> &ParseResult {
        &self.parsed
    }

    pub(super) fn revision(&self) -> &[u8] {
        &self.revision
    }

    pub(super) fn title(&self) -> String {
        self.parsed
            .document()
            .header
            .as_ref()
            .map(|h| render::inline_text(&h.title, Format::Plain))
            .unwrap_or_default()
    }

    pub(super) fn source(&self, location: &Location) -> Value {
        let start = self.parsed.source_location(location);
        let mut end_location = location.clone();
        end_location.start = location.end.clone();
        let end = self.parsed.source_location(&end_location);
        json!({
            "file": canonical_source(start.file.as_ref().unwrap_or(&self.path)),
            "line": location.start.line, "column": location.start.column,
            "end_file": canonical_source(end.file.as_ref().unwrap_or(&self.path)),
            "end_line": location.end.line, "end_column": location.end.column,
        })
    }

    pub(super) fn warnings(&self) -> Vec<Value> {
        self.parsed.warnings().iter().map(|warning| {
            let source = warning.source_location().map(|source| json!({
                "file": canonical_source(source.file.as_ref().unwrap_or(&self.path)),
                "line": source.location.start.line, "column": source.location.start.column,
            }));
            json!({"kind":"warning", "path": self.path, "message": warning.to_string(), "source": source})
        }).collect()
    }

    pub(super) fn entries(&self) -> Result<Vec<Entry<'_>>> {
        let mut entries = Vec::new();
        for (index, block) in self.parsed.document().blocks.iter().enumerate() {
            walk(block, &index.to_string(), &[], 0, &mut entries)?;
        }
        Ok(entries)
    }

    pub(super) fn describe(&self, entry: &Entry<'_>) -> Value {
        json!({
            "kind": render::kind(entry.block), "path": self.path,
            "selector": entry.selector, "id": entry.id(), "title": entry.title(),
            "heading_path": entry.heading_path, "source": self.source(entry.block.location()),
        })
    }
}

pub(super) struct Entry<'a> {
    block: &'a Block<'a>,
    selector: String,
    heading_path: Vec<String>,
}

impl<'a> Entry<'a> {
    pub(super) fn block(&self) -> &'a Block<'a> {
        self.block
    }

    pub(super) fn selector(&self) -> &str {
        &self.selector
    }

    fn id(&self) -> Option<String> {
        if let Block::Section(section) = self.block {
            return Some(section.id().into_owned());
        }
        self.block
            .metadata()
            .and_then(|metadata| metadata.id.as_ref())
            .map(|anchor| anchor.id.to_owned())
    }

    pub(super) fn title(&self) -> String {
        render::title(self.block)
            .map(|title| render::inline_text(title, Format::Plain))
            .unwrap_or_default()
    }

    pub(super) fn has_id(&self, id: &str) -> bool {
        self.id().as_deref() == Some(id)
            || self
                .block
                .metadata()
                .is_some_and(|metadata| metadata.anchors.iter().any(|anchor| anchor.id == id))
    }

    pub(super) fn navigable(&self) -> bool {
        matches!(self.block, Block::Section(_))
            || self.id().is_some()
            || !self.title().is_empty()
            || matches!(render::kind(self.block), "listing" | "literal" | "table")
    }
}

fn walk<'a>(
    block: &'a Block<'a>,
    position: &str,
    trail: &[String],
    depth: usize,
    entries: &mut Vec<Entry<'a>>,
) -> Result<()> {
    if depth > 64 || entries.len() >= 50_000 {
        return Err(Error::Input(
            "document exceeds navigation depth or block limit".into(),
        ));
    }
    let mut trail = trail.to_vec();
    let kind = if let Block::Section(section) = block {
        trail.push(render::inline_text(&section.title, Format::Plain));
        "section"
    } else {
        "block"
    };
    entries.push(Entry {
        block,
        selector: format!("@{kind}:{position}"),
        heading_path: trail.clone(),
    });
    for (index, child) in render::children(block).into_iter().enumerate() {
        walk(
            child,
            &format!("{position}.{index}"),
            &trail,
            depth + 1,
            entries,
        )?;
    }
    Ok(())
}

fn canonical_source(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

pub(super) fn select<'a>(
    entries: &'a [Entry<'a>],
    selector: &str,
    section_only: bool,
) -> Vec<&'a Entry<'a>> {
    let available = entries
        .iter()
        .filter(|entry| matches!(entry.block, Block::Section(_)) == section_only)
        .collect::<Vec<_>>();
    let exact = available
        .iter()
        .copied()
        .filter(|entry| {
            entry.selector == selector || entry.has_id(selector.trim_start_matches('#'))
        })
        .collect::<Vec<_>>();
    if !exact.is_empty() {
        return exact;
    }
    let heading_path = selector.split('>').map(str::trim).collect::<Vec<_>>();
    if heading_path.len() > 1 {
        return available
            .into_iter()
            .filter(|entry| {
                let mut path = entry.heading_path.clone();
                if !section_only {
                    path.push(entry.title());
                }
                path.len() >= heading_path.len()
                    && path[path.len() - heading_path.len()..]
                        .iter()
                        .zip(&heading_path)
                        .all(|(actual, requested)| {
                            actual.to_lowercase() == requested.to_lowercase()
                        })
            })
            .collect();
    }
    let pattern = Pattern::new(
        selector.trim(),
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::default();
    let mut buffer = Vec::new();
    let ranked = available
        .into_iter()
        .filter_map(|entry| {
            pattern
                .score(Utf32Str::new(&entry.title(), &mut buffer), &mut matcher)
                .map(|score| (entry, score))
        })
        .collect::<Vec<_>>();
    let best = ranked.iter().map(|(_, score)| *score).max();
    ranked
        .into_iter()
        .filter(|(_, score)| Some(*score) == best)
        .map(|(entry, _)| entry)
        .collect()
}

fn include_loader(
    config: Config,
    sources: Arc<Mutex<Sources>>,
    cancelled: CancellationToken,
) -> IncludeLoader {
    IncludeLoader::custom(move |target: &IncludeSourceTarget| {
        let fatal =
            |message: String| IncludeSourceError::new(IncludeSourceErrorKind::Fatal, message);
        if cancelled.is_cancelled() {
            return Err(fatal("request cancelled".into()));
        }
        match target {
            IncludeSourceTarget::File(path) => {
                let (path, bytes) = config.read(path).map_err(|error| {
                    IncludeSourceError::new(IncludeSourceErrorKind::Unavailable, error.to_string())
                })?;
                let mut sources = sources
                    .lock()
                    .map_err(|_| fatal("source lock poisoned".into()))?;
                sources.total_bytes += bytes.len();
                if sources.total_bytes > MAX_ASSEMBLED_BYTES {
                    return Err(fatal("include reads exceed 16 MiB".into()));
                }
                sources.hashes.insert(path, Sha256::digest(&bytes).to_vec());
                Ok(IncludeSource::from_bytes(bytes))
            }
            _ => Err(IncludeSourceError::new(
                IncludeSourceErrorKind::Unsupported,
                "remote includes are disabled",
            )),
        }
    })
}
