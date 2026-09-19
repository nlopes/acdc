//! The diagram source a converter is handed.
//!
//! A `DiagramSource` bundles the diagram code with everything a generator
//! needs to decide how to render it: the block's own attributes, the document
//! attributes they fall back to, the directory relative paths resolve against,
//! and the cache of tool locations that is shared across a whole document.
//!
//! # Attribute lookup
//!
//! asciidoctor-diagram gives each attribute three chances to be set, and the
//! generators rely on all three:
//!
//! | Lookup | Block attribute | Document fallback |
//! |---|---|---|
//! | [`attr`](DiagramSource::attr) | `layout=dot` | `:graphviz-layout:` |
//! | [`global_attr`](DiagramSource::global_attr) | `format=svg` | `:diagram-format:` |
//! | [`doc_attr`](DiagramSource::doc_attr) | `dot=/usr/bin/dot` | `:dot:` |
//!
//! `attr` is the per-diagram-type form used by tool-specific options,
//! `global_attr` is the cross-diagram form used by processor-wide settings
//! (`format`, `cachedir`, `on-error`), and `doc_attr` is the plain form used
//! for things that are not diagram-specific at all, such as tool paths.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    path::{Path, PathBuf},
};

use acdc_parser::{AttributeValue, DocumentAttributes};
use sha2::{Digest, Sha256};

use crate::{
    cache::ImageMetadata,
    error::{Error, Result},
    paths,
    which::{is_usable_command_path, which},
};

/// Tool locations resolved so far, keyed by the attribute that names the tool.
///
/// Looking a tool up means stat-ing every `PATH` entry; a document with thirty
/// `PlantUML` blocks should pay for that once.
pub(crate) type CommandCache = RefCell<HashMap<String, Option<PathBuf>>>;

/// Where a diagram's code came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Origin {
    /// The body of a `[graphviz]`-style block.
    Block,
    /// A file named by a `graphviz::diagram.dot[]` block macro.
    File(PathBuf),
}

/// Everything needed to render one diagram block.
///
/// Assembled by the AST walk and consumed by [`DiagramSource::new`].
pub(crate) struct Request<'a> {
    /// The registered diagram name.
    pub(crate) name: &'static str,
    /// The diagram code.
    pub(crate) code: String,
    /// The block's attributes, with lower-case names.
    pub(crate) attributes: BTreeMap<String, String>,
    /// The block's options (`[graphviz%nocache]`).
    pub(crate) options: Vec<String>,
    /// Where the code came from.
    pub(crate) origin: Origin,
    /// The directory relative paths in this block resolve against.
    pub(crate) base_dir: PathBuf,
    /// Document attributes, for attribute fallbacks.
    pub(crate) document: &'a DocumentAttributes<'a>,
    /// Tool locations resolved so far.
    pub(crate) commands: &'a CommandCache,
}

/// One diagram, ready to be handed to a converter.
pub(crate) struct DiagramSource<'doc> {
    diagram_type: &'static str,
    attributes: BTreeMap<String, String>,
    options: Vec<String>,
    document: &'doc DocumentAttributes<'doc>,
    base_dir: PathBuf,
    origin: Origin,
    code: String,
    commands: &'doc CommandCache,
    unsafe_mode: bool,
}

impl<'doc> DiagramSource<'doc> {
    /// Assemble a source from a render request.
    pub(crate) fn new(request: Request<'doc>, unsafe_mode: bool) -> Self {
        let Request {
            name,
            code,
            attributes,
            options,
            origin,
            base_dir,
            document,
            commands,
        } = request;
        Self {
            diagram_type: name,
            attributes,
            options,
            document,
            base_dir,
            origin,
            code,
            commands,
            unsafe_mode,
        }
    }

    /// Whether the document is being processed with no safe-mode restrictions.
    ///
    /// Generators that run author-supplied shell commands (`[tape]`) refuse to
    /// do so unless this holds.
    pub(crate) fn unsafe_mode(&self) -> bool {
        self.unsafe_mode
    }

    /// The registered name of this diagram type, such as `graphviz`.
    pub(crate) fn diagram_type(&self) -> &'static str {
        self.diagram_type
    }

    /// The diagram code.
    pub(crate) fn code(&self) -> &str {
        &self.code
    }

    /// Replace the diagram code, as `PlantUML` does when it wraps the body in
    /// `@startuml`/`@enduml` and runs the preprocessor over it.
    pub(crate) fn set_code(&mut self, code: String) {
        self.code = code;
    }

    /// The directory relative paths in this diagram resolve against: the
    /// directory holding the source file for a block macro, otherwise the
    /// document's own directory.
    pub(crate) fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Take the attributes, leaving the source with none.
    ///
    /// Called once the diagram has been generated: what is left over is
    /// exactly the set of attributes that belong on the replacement node.
    pub(crate) fn take_attributes(&mut self) -> BTreeMap<String, String> {
        std::mem::take(&mut self.attributes)
    }

    /// Remove an attribute, returning its value.
    ///
    /// Attributes that control generation (`format`, `title`, `caption`) are
    /// consumed rather than read so they do not end up on the `<img>` tag.
    pub(crate) fn take_attribute(&mut self, name: &str) -> Option<String> {
        self.attributes.remove(name)
    }

    /// Look up a diagram-type attribute: the block's own, else
    /// `{diagram-type}-{name}` on the document.
    ///
    /// Several names can be given for attributes that are spelled more than
    /// one way (`ganttconfig` and `gantt-config`); the first one set wins.
    pub(crate) fn attr(&self, names: &[&str]) -> Option<String> {
        for name in names {
            if let Some(value) = self.attributes.get(*name) {
                return Some(value.clone());
            }
        }
        for name in names {
            if let Some(value) = self.document_string(&format!("{}-{name}", self.diagram_type)) {
                return Some(value);
            }
        }
        None
    }

    /// Look up an attribute that falls back to a plain document attribute,
    /// used for values that are not diagram-specific (tool paths, `data-uri`).
    pub(crate) fn doc_attr(&self, name: &str) -> Option<String> {
        self.attributes
            .get(name)
            .cloned()
            .or_else(|| self.document_string(name))
    }

    /// Look up a processor-wide attribute: the block's own, else
    /// `diagram-{name}` on the document.
    pub(crate) fn global_attr(&self, name: &str) -> Option<String> {
        self.attributes
            .get(name)
            .cloned()
            .or_else(|| self.document_string(&format!("diagram-{name}")))
    }

    /// Whether a diagram-type option is set, as `[graphviz%nocache]`,
    /// `nocache-option=`, or `:graphviz-nocache-option:`.
    pub(crate) fn opt(&self, name: &str) -> bool {
        self.options.iter().any(|option| option == name)
            || self.attr(&[&format!("{name}-option")]).is_some()
    }

    /// Whether a processor-wide option is set, as `[graphviz%nocache]` or
    /// `:diagram-nocache-option:`.
    pub(crate) fn global_opt(&self, name: &str) -> bool {
        self.options.iter().any(|option| option == name)
            || self.global_attr(&format!("{name}-option")).is_some()
    }

    /// Resolve `target` against `start`, defaulting to this diagram's base
    /// directory.
    pub(crate) fn resolve_path(&self, target: &str, start: Option<&Path>) -> PathBuf {
        paths::resolve(Path::new(target), start.unwrap_or(&self.base_dir))
    }

    /// The base name of the generated image, without its extension.
    ///
    /// An explicit `target` wins; a block macro otherwise names the image
    /// after its source file, and an inline block falls back to its checksum
    /// so that identical diagrams share a file.
    pub(crate) fn image_name(&self) -> String {
        if let Some(target) = self.attributes.get("target") {
            return target.clone();
        }
        match &self.origin {
            Origin::File(path) => path.file_stem().map_or_else(
                || self.checksum(),
                |stem| stem.to_string_lossy().into_owned(),
            ),
            Origin::Block => format!("diag-{}", self.checksum()),
        }
    }

    /// A stable digest of everything that affects the generated image.
    ///
    /// Both the code and the block attributes go in, so changing `layout=neato`
    /// invalidates the cache even though the diagram text is untouched. The
    /// diagram type prefixes the digest so the same code rendered by two tools
    /// never collides.
    ///
    /// asciidoctor-diagram uses MD5 here; acdc uses the leading half of a
    /// SHA-256 digest, which keeps file names a comparable length without
    /// depending on a broken hash. Cache files written by one tool are
    /// therefore not reused by the other.
    pub(crate) fn checksum(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.code.as_bytes());
        // `attributes` is a BTreeMap, so this walk is in name order and the
        // digest does not depend on how the attribute list was written.
        for (name, value) in &self.attributes {
            hasher.update(name.as_bytes());
            hasher.update(b"\0");
            hasher.update(value.as_bytes());
            hasher.update(b"\0");
        }
        let digest = hasher.finalize();
        let mut checksum = format!("{}-sha256-", self.diagram_type);
        for byte in digest.iter().take(16) {
            let _ = write!(checksum, "{byte:02x}");
        }
        checksum
    }

    /// Whether the cached image at `image_file` is stale.
    ///
    /// The checksum catches edits to the block; for a block macro the source
    /// file's timestamp catches edits to a file the document only references.
    pub(crate) fn should_process(&self, image_file: &Path, metadata: &ImageMetadata) -> bool {
        if let Origin::File(path) = &self.origin
            && let (Some(source_time), Some(image_time)) =
                (paths::modified(path), paths::modified(image_file))
            && source_time > image_time
        {
            return true;
        }
        metadata.checksum.as_deref() != Some(self.checksum().as_str())
    }

    /// The metadata to store alongside a freshly generated image.
    pub(crate) fn image_metadata(&self) -> ImageMetadata {
        ImageMetadata {
            checksum: Some(self.checksum()),
            ..ImageMetadata::default()
        }
    }

    /// Locate a diagram tool, returning `None` rather than failing.
    ///
    /// Used where a missing tool is a routine branch instead of an error:
    /// `PlantUML` falls back to `Smetana` when `Graphviz` is absent, and Mermaid
    /// tries `mmdc` before the older `mermaid`.
    pub(crate) fn find_command_opt(&self, lookup: &CommandLookup<'_>) -> Option<PathBuf> {
        let key = lookup.cache_key();
        if let Some(cached) = self.commands.borrow().get(&key) {
            return cached.clone();
        }
        let found = self.search_command(lookup);
        self.commands.borrow_mut().insert(key, found.clone());
        found
    }

    /// Locate a diagram tool, failing with advice on how to point acdc at it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CommandNotFound`] when neither an attribute nor `PATH`
    /// turns up an executable.
    pub(crate) fn find_command(&self, lookup: &CommandLookup<'_>) -> Result<PathBuf> {
        self.find_command_opt(lookup)
            .ok_or_else(|| Error::CommandNotFound {
                commands: lookup.commands.iter().map(|c| (*c).to_string()).collect(),
                attribute: lookup.primary_attribute().to_string(),
            })
    }

    fn search_command(&self, lookup: &CommandLookup<'_>) -> Option<PathBuf> {
        // An attribute pointing straight at the executable wins, so a document
        // can pin a specific build without touching PATH.
        for name in lookup.attributes() {
            if let Some(value) = self.doc_attr(&name) {
                let candidate = paths::resolve(Path::new(&value), &self.base_dir);
                if is_usable_command_path(&candidate) {
                    tracing::debug!(attribute = %name, path = %candidate.display(), "diagram tool set by attribute");
                    return Some(candidate);
                }
                tracing::debug!(attribute = %name, path = %candidate.display(), "attribute does not name an executable");
            }
        }

        for command in lookup.commands {
            if let Some(path) = which(command, lookup.extra_paths) {
                tracing::debug!(command, path = %path.display(), "found diagram tool on PATH");
                return Some(path);
            }
        }
        None
    }

    fn document_string(&self, name: &str) -> Option<String> {
        match self.document.get(name)? {
            AttributeValue::String(value) => Some(strip_quotes(value).to_string()),
            AttributeValue::Bool(set) => set.then(|| "true".to_string()),
            AttributeValue::None => None,
            // `AttributeValue` is non-exhaustive: a value kind added later is
            // not something a diagram tool can be handed.
            other => {
                tracing::debug!(
                    ?other,
                    attribute = name,
                    "ignoring unsupported attribute value"
                );
                None
            }
        }
    }
}

/// How to find one diagram tool.
///
/// Mirrors asciidoctor-diagram's `find_command`: a list of document attributes
/// that may point at the executable, a list of names it ships under, and extra
/// directories to search beyond `PATH` (macOS application bundles).
pub(crate) struct CommandLookup<'a> {
    /// Executable names, most preferred first.
    pub(crate) commands: &'a [&'a str],
    /// Attribute names checked before `PATH`. Empty means "the first command
    /// name", which is what nearly every tool wants.
    pub(crate) attributes: &'a [&'a str],
    /// Directories searched before `PATH`.
    pub(crate) extra_paths: &'a [PathBuf],
}

impl<'a> CommandLookup<'a> {
    /// A tool found by its own name, with no alternatives.
    pub(crate) const fn new(commands: &'a [&'a str]) -> Self {
        Self {
            commands,
            attributes: &[],
            extra_paths: &[],
        }
    }

    /// Also honour these document attributes when locating the tool.
    pub(crate) const fn with_attributes(mut self, attributes: &'a [&'a str]) -> Self {
        self.attributes = attributes;
        self
    }

    /// Also search these directories before `PATH`.
    pub(crate) const fn with_paths(mut self, extra_paths: &'a [PathBuf]) -> Self {
        self.extra_paths = extra_paths;
        self
    }

    /// Attribute names to try, in order: the explicit ones, then the primary
    /// command name.
    fn attributes(&self) -> Vec<String> {
        let mut names: Vec<String> = self.attributes.iter().map(|a| (*a).to_string()).collect();
        if let Some(primary) = self.commands.first() {
            let primary = (*primary).to_string();
            if !names.contains(&primary) {
                names.push(primary);
            }
        }
        names
    }

    /// The attribute named in the "tool not found" message.
    fn primary_attribute(&self) -> &str {
        self.attributes
            .first()
            .or_else(|| self.commands.first())
            .copied()
            .unwrap_or("")
    }

    fn cache_key(&self) -> String {
        format!("cmd-{}", self.primary_attribute())
    }
}

/// Strip one layer of matching quotes, which the parser can leave on values.
fn strip_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    match (bytes.first(), bytes.last()) {
        (Some(b'"'), Some(b'"')) | (Some(b'\''), Some(b'\'')) if value.len() >= 2 => {
            &value[1..value.len() - 1]
        }
        _ => value,
    }
}
