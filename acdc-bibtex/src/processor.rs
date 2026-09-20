//! The pass that resolves a document's citations.
//!
//! asciidoctor-bibtex does this in a treeprocessor, in three passes over the
//! document: gather every cited key, put them in order, then replace the
//! macros and the `bibliography::[]` placeholder. The same three passes run
//! here, over acdc's AST, between parsing and conversion — so every backend
//! renders ordinary text and links, and none of them needs to know a citation
//! was involved.
//!
//! The order matters: a numeric style cites a work by its position in the
//! bibliography, so no citation can be rendered until every key is known.

use std::path::{Path, PathBuf};

use acdc_converters_core::{Diagnostics, Warning, WarningSource};
use acdc_parser::{
    Anchor, Block, CrossReference, Document, DocumentArena, Form, Highlight, InlineMacro,
    InlineNode, Italic, Link, Location, Paragraph, Plain, Raw, Source, SourceLocation, SourceUrl,
};

use crate::{
    database::{Database, Entry},
    error::Error,
    macros::{self, Call, Item, Kind},
    names,
    settings::{Format, MacroDefaults, Settings},
    style::{Span, Style},
    walk,
};

/// The text `bibliography::[]` leaves behind for the pass to replace.
const BIBLIOGRAPHY_MACRO: &str = "bibliography::";

/// Configuration for one run of the pass.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Options {
    base_dir: PathBuf,
}

impl Default for Options {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl Options {
    /// Start building a configuration.
    #[must_use]
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }

    /// The directory a relative `bibtex-file` resolves against, and the one
    /// searched when the document names no file.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

/// Builder for [`Options`].
#[derive(Debug, Clone, Default)]
pub struct OptionsBuilder {
    base_dir: Option<PathBuf>,
}

impl OptionsBuilder {
    /// Set the directory relative paths resolve against, normally the one
    /// holding the document.
    #[must_use]
    pub fn base_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(self) -> Options {
        Options {
            base_dir: self
                .base_dir
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        }
    }
}

/// Resolves citations against a BibTeX database.
#[derive(Debug)]
pub struct Processor {
    options: Options,
    warning_source: WarningSource,
}

impl Processor {
    /// Build a processor.
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self {
            options,
            warning_source: WarningSource::new("bibtex"),
        }
    }

    /// The configuration this processor was built with.
    #[must_use]
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Replace every citation macro in `document`, and build its bibliography.
    ///
    /// A document that cites nothing and asks for no bibliography is left
    /// alone, and needs no database. A key the database does not hold is
    /// reported and rendered as itself, unless `:bibtex-throw: true` asks for
    /// it to be an error.
    ///
    /// # Errors
    ///
    /// Returns an error when the document cites something but no database can
    /// be read, when the database is malformed, or when an unknown key is
    /// fatal by request.
    pub fn process<'arena>(
        &self,
        document: &mut Document<'arena>,
        arena: &'arena DocumentArena,
        warnings: &mut Vec<Warning>,
    ) -> Result<(), Error> {
        let settings = Settings::read(&document.attributes, &macro_defaults(&document.blocks));
        let mut diagnostics = Diagnostics::new(&self.warning_source, warnings);

        let found = gather(document);
        let placeholders = count_placeholders(&document.blocks);
        if found.cited.is_empty() && found.bibitems == 0 && placeholders == 0 {
            return Ok(());
        }
        if let Some(name) = &settings.requested_style
            && Style::parse(name).is_none()
        {
            diagnostics.warn_with_advice(
                format!(
                    "unknown bibtex style `{name}`; using `{}`",
                    settings.style.as_str()
                ),
                format!("Known styles: {}.", crate::style::NAMES.join(", ")),
            );
        }

        let database = self.load(&settings)?;
        let keys = order(found.cited, &database, &settings);

        replace_citations(
            document,
            &Context {
                settings: &settings,
                database: &database,
                keys: &keys,
                arena,
            },
            &mut diagnostics,
        )?;
        replace_bibliography(
            &mut document.blocks,
            &Context {
                settings: &settings,
                database: &database,
                keys: &keys,
                arena,
            },
        );
        Ok(())
    }

    /// Read the database the document names, or the one beside it.
    fn load(&self, settings: &Settings) -> Result<Database, Error> {
        let path = match &settings.file {
            Some(file) => {
                let named = Path::new(file);
                if named.is_absolute() {
                    named.to_path_buf()
                } else {
                    self.options.base_dir.join(named)
                }
            }
            None => find_database(&self.options.base_dir).ok_or(Error::NoDatabase)?,
        };
        let source = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        Database::parse(&source)
    }
}

/// Everything the replacement passes need.
struct Context<'ctx, 'arena> {
    settings: &'ctx Settings,
    database: &'ctx Database,
    /// Every cited key, in bibliography order.
    keys: &'ctx [String],
    arena: &'arena DocumentArena,
}

impl Context<'_, '_> {
    /// A key's position in the bibliography, counting from one.
    fn number(&self, key: &str) -> usize {
        self.keys
            .iter()
            .position(|k| k == key)
            .map_or(0, |at| at + 1)
    }
}

/// The only `.bib` file in a directory, when there is exactly one.
///
/// asciidoctor-bibtex takes the first of however many it finds; picking one of
/// several at random is a silent surprise, so an ambiguous directory is left
/// to the document to resolve with `:bibtex-file:`.
fn find_database(directory: &Path) -> Option<PathBuf> {
    let mut found = None;
    for entry in std::fs::read_dir(directory).ok()?.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "bib") {
            if found.is_some() {
                return None;
            }
            found = Some(path);
        }
    }
    found
}

/// What a first pass over the document found.
struct Found {
    /// Every cited key, in the order the document first mentions it.
    cited: Vec<String>,
    /// How many `bibitem:[]` macros there are.
    ///
    /// A bibitem is rendered where it stands and never joins the
    /// bibliography, but a document that has nothing else still needs its
    /// database read — which is how a CV built out of bibitems works.
    bibitems: usize,
}

/// Find every citation and bibitem the document holds.
fn gather(document: &mut Document<'_>) -> Found {
    let mut found = Found {
        cited: Vec::new(),
        bibitems: 0,
    };
    walk::document(document, &mut |nodes| {
        for node in nodes.iter() {
            let InlineNode::PlainText(text) = node else {
                continue;
            };
            for item in macros::find_all(text.content) {
                match item.macro_call {
                    Call::Citation { items, .. } => {
                        found.cited.extend(items.into_iter().map(|item| item.key));
                    }
                    Call::Bibitem { .. } => found.bibitems += 1,
                }
            }
        }
    });
    found
}

/// Put the cited keys into bibliography order.
fn order(cited: Vec<String>, database: &Database, settings: &Settings) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for key in cited {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    if settings.in_appearance_order() {
        return keys;
    }
    // Sorted by the credited names as a bibliography files them, then year —
    // upper-cased so that case does not separate two spellings of a name.
    keys.sort_by_cached_key(|key| {
        let Some(entry) = database.get(key) else {
            return (vec![key.to_uppercase()], String::new());
        };
        let names: Vec<String> = names::parse_list(entry.creators().unwrap_or_default())
            .iter()
            .map(|name| name.family_first().to_uppercase())
            .collect();
        (names, entry.field("year").unwrap_or_default().to_string())
    });
    keys
}

/// How many `bibliography::[]` placeholders the document holds.
fn count_placeholders(blocks: &[Block<'_>]) -> usize {
    let mut total = 0;
    for_each_bibliography_macro(blocks, &mut |_| total += 1);
    total
}

/// What the first `bibliography::[]` call names, for settings the header did
/// not already give.
fn macro_defaults(blocks: &[Block<'_>]) -> MacroDefaults {
    let mut defaults = MacroDefaults::default();
    let mut seen = false;
    for_each_bibliography_macro(blocks, &mut |line| {
        if !seen {
            seen = true;
            defaults = parse_bibliography_macro(line);
        }
    });
    defaults
}

/// Visit every `bibliography::[]` call, in document order.
fn for_each_bibliography_macro(blocks: &[Block<'_>], visit: &mut impl FnMut(&str)) {
    for block in blocks {
        if let Some(line) = bibliography_macro(block) {
            visit(line);
        }
        if let Block::Section(section) = block {
            for_each_bibliography_macro(&section.content, visit);
        }
    }
}

/// Read the target and style out of a `bibliography::target[style,locale]`
/// call.
///
/// The locale is accepted and ignored: acdc renders the three styles it
/// supports in English only.
fn parse_bibliography_macro(line: &str) -> MacroDefaults {
    let Some(rest) = line.strip_prefix(BIBLIOGRAPHY_MACRO) else {
        return MacroDefaults::default();
    };
    let Some((target, attributes)) = rest.split_once('[') else {
        return MacroDefaults::default();
    };
    let attributes = attributes.strip_suffix(']').unwrap_or(attributes);

    let mut style = None;
    for (index, attribute) in attributes.split(',').map(str::trim).enumerate() {
        match attribute.split_once('=') {
            Some(("style", value)) => style = Some(value.trim()),
            // `style` is the first positional attribute, `locale` the second.
            None if index == 0 => style = Some(attribute),
            _ => {}
        }
    }

    MacroDefaults {
        file: non_empty(target.trim()),
        style: style.and_then(non_empty),
    }
}

/// A trimmed value, unless there was nothing to it.
fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

/// The `bibliography::…[]` call a block is, if it is one.
fn bibliography_macro<'b>(block: &'b Block<'_>) -> Option<&'b str> {
    let Block::Paragraph(paragraph) = block else {
        return None;
    };
    let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
        return None;
    };
    let line = text.content.trim();
    line.starts_with(BIBLIOGRAPHY_MACRO)
        .then_some(line)
        .filter(|line| line.ends_with(']'))
}

/// Replace every citation and bibitem macro with what it stands for.
fn replace_citations<'arena>(
    document: &mut Document<'arena>,
    context: &Context<'_, 'arena>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<(), Error> {
    let mut failure = None;
    walk::document(document, &mut |nodes| {
        if failure.is_some() {
            return;
        }
        let mut rebuilt = Vec::with_capacity(nodes.len());
        for node in std::mem::take(nodes) {
            let InlineNode::PlainText(text) = &node else {
                rebuilt.push(node);
                continue;
            };
            let found = macros::find_all(text.content);
            if found.is_empty() {
                rebuilt.push(node);
                continue;
            }

            let mut cursor = 0;
            for item in found {
                if let Some(before) = text.content.get(cursor..item.span.start) {
                    push_source_text(&mut rebuilt, context.arena, before, text);
                }
                cursor = item.span.end;
                match expand(&item.macro_call, context, &text.location, diagnostics) {
                    Ok(nodes) => rebuilt.extend(nodes),
                    Err(error) => {
                        failure = Some(error);
                        return;
                    }
                }
            }
            if let Some(rest) = text.content.get(cursor..) {
                push_source_text(&mut rebuilt, context.arena, rest, text);
            }
        }
        *nodes = rebuilt;
    });
    failure.map_or(Ok(()), Err)
}

/// Turn one macro into the nodes that replace it.
fn expand<'arena>(
    call: &Call,
    context: &Context<'_, 'arena>,
    location: &Location,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<Vec<InlineNode<'arena>>, Error> {
    match call {
        Call::Bibitem { key } => {
            let Some(entry) = context.database.get(key) else {
                report_unknown(key, context, location, diagnostics)?;
                return Ok(vec![text_node(context.arena, key, location)]);
            };
            // A bibitem stands alone in the text, with nothing before it to
            // shorten a repeated name against.
            Ok(spans_to_nodes(
                &context.settings.style.bibliography(entry, None),
                context.arena,
                location,
            ))
        }
        Call::Citation {
            kind,
            pretext,
            items,
        } => citation(*kind, pretext, items, context, location, diagnostics),
    }
}

/// Build the nodes for one citation macro.
fn citation<'arena>(
    kind: Kind,
    pretext: &str,
    items: &[Item],
    context: &Context<'_, 'arena>,
    location: &Location,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<Vec<InlineNode<'arena>>, Error> {
    if context.settings.format != Format::Asciidoc {
        return Ok(vec![passthrough(kind, items, context, location)]);
    }

    let style = context.settings.style;
    let numeric = style.is_numeric();
    let (open, close) = if numeric {
        (
            context.settings.open.as_str(),
            context.settings.close.as_str(),
        )
    } else if kind == Kind::Parenthetical {
        ("(", ")")
    } else {
        ("", "")
    };

    let mut inner: Vec<InlineNode<'arena>> = Vec::new();
    // A numeric style puts the pretext outside the brackets, an author-date
    // style inside them, which is where the reader expects "See" to sit.
    let mut leading = String::new();
    if !pretext.is_empty() {
        leading.push_str(pretext);
        leading.push(' ');
    }

    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            push_text(
                &mut inner,
                context.arena,
                &format!("{} ", style.item_separator()),
                location,
            );
        }
        let Some(entry) = context.database.get(&item.key) else {
            report_unknown(&item.key, context, location, diagnostics)?;
            push_text(&mut inner, context.arena, &item.key, location);
            continue;
        };
        let label = format!(
            "{}{}",
            citation_label(style, entry, &item.key, context),
            style.locator(&item.locator)
        );
        let label = if kind == Kind::Narrative && !numeric {
            narrative(&label, entry)
        } else {
            label
        };
        inner.push(link(context.arena, &item.key, &label, location));
    }

    let mut rendered: Vec<InlineNode<'arena>> = Vec::new();
    if numeric {
        push_text(
            &mut rendered,
            context.arena,
            &format!("{leading}{open}"),
            location,
        );
    } else {
        push_text(
            &mut rendered,
            context.arena,
            &format!("{open}{leading}"),
            location,
        );
    }
    rendered.extend(inner);
    push_text(&mut rendered, context.arena, close, location);

    Ok(vec![InlineNode::HighlightText(Highlight {
        role: Some(context.arena.alloc_str("citation")),
        id: None,
        form: Form::Constrained,
        content: rendered,
        location: location.clone(),
    })])
}

/// The text a citation shows for one entry, before its locator.
fn citation_label(style: Style, entry: &Entry, key: &str, context: &Context<'_, '_>) -> String {
    if style.is_numeric() {
        return context.number(key).to_string();
    }
    style.citation(entry)
}

/// Move the year into parentheses so the names read as part of the sentence.
///
/// `Lane, 2000, p. 89` becomes `Lane (2000, p. 89)`, which is what `citenp`
/// is for.
fn narrative(label: &str, entry: &Entry) -> String {
    let Some(year) = entry.field("year").filter(|year| !year.is_empty()) else {
        return label.to_string();
    };
    let Some(at) = label.find(year) else {
        return label.to_string();
    };
    let (head, tail) = label.split_at(at);
    format!("{}({tail})", head.replace(", ", " "))
}

/// A `\cite{…}` passthrough for a LaTeX toolchain.
fn passthrough<'arena>(
    kind: Kind,
    items: &[Item],
    context: &Context<'_, 'arena>,
    location: &Location,
) -> InlineNode<'arena> {
    let command = match (context.settings.format, kind) {
        (Format::Biblatex, Kind::Narrative) => "textcite",
        (Format::Biblatex, Kind::Parenthetical) => "parencite",
        // xelatex has no `\citenp`, so both forms cite the same way.
        _ => "cite",
    };
    let body = items
        .iter()
        .map(|item| {
            let locator = if item.locator.is_empty() {
                String::new()
            } else {
                format!("[p. {}]", item.locator)
            };
            format!("\\{command}{locator}{{{}}}", item.key)
        })
        .collect::<Vec<_>>()
        .join(",");
    InlineNode::RawText(Raw {
        content: context.arena.alloc_str(&body),
        location: location.clone(),
        subs: Vec::new(),
    })
}

/// Report a key the database does not hold, or fail if the document asked to.
fn report_unknown(
    key: &str,
    context: &Context<'_, '_>,
    location: &Location,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<(), Error> {
    if context.settings.throw {
        return Err(Error::UnknownReference(key.to_string()));
    }
    diagnostics.emit(
        Warning::new(
            diagnostics.source().clone(),
            format!("unknown reference: {key}"),
        )
        .with_advice("The key is rendered as written. Check it against the bibtex database.")
        .at(SourceLocation::at_location(None, location.clone())),
    );
    Ok(())
}

/// Replace the `bibliography::[]` placeholders with the reference list.
fn replace_bibliography<'arena>(blocks: &mut Vec<Block<'arena>>, context: &Context<'_, 'arena>) {
    let mut index = 0;
    while index < blocks.len() {
        if let Some(Block::Section(section)) = blocks.get_mut(index) {
            replace_bibliography(&mut section.content, context);
            index += 1;
            continue;
        }
        if blocks.get(index).and_then(bibliography_macro).is_none() {
            index += 1;
            continue;
        }
        let location = match blocks.get(index) {
            Some(Block::Paragraph(paragraph)) => paragraph.location.clone(),
            _ => Location::default(),
        };
        let entries = bibliography_blocks(context, &location);
        let count = entries.len();
        blocks.splice(index..=index, entries);
        index += count;
    }
}

/// One paragraph per bibliography entry, in order.
fn bibliography_blocks<'arena>(
    context: &Context<'_, 'arena>,
    location: &Location,
) -> Vec<Block<'arena>> {
    // A LaTeX toolchain builds the list itself, so all that is left here is
    // the command that tells it to.
    let commands: Vec<String> = match context.settings.format {
        Format::Biblatex => vec!["\\printbibliography".to_string()],
        Format::Bibtex => vec![
            format!(
                "\\bibliography{{{}}}{{}}",
                context.settings.file.as_deref().unwrap_or_default()
            ),
            format!("\\bibliographystyle{{{}}}", context.settings.style.as_str()),
        ],
        Format::Asciidoc => Vec::new(),
    };
    if !commands.is_empty() {
        return commands
            .iter()
            .map(|command| {
                paragraph(
                    vec![InlineNode::RawText(Raw {
                        content: context.arena.alloc_str(command),
                        location: location.clone(),
                        subs: Vec::new(),
                    })],
                    location,
                )
            })
            .collect();
    }

    context
        .keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let mut rendered = vec![InlineNode::InlineAnchor(Anchor::new(
                context.arena.alloc_str(key),
                location.clone(),
            ))];
            if context.settings.style.is_numeric() {
                push_text(
                    &mut rendered,
                    context.arena,
                    &format!(
                        "{}{}{} ",
                        context.settings.open,
                        index + 1,
                        context.settings.close
                    ),
                    location,
                );
            }
            let previous = index
                .checked_sub(1)
                .and_then(|before| context.keys.get(before))
                .and_then(|before| context.database.get(before));
            match context.database.get(key) {
                Some(entry) => rendered.extend(spans_to_nodes(
                    &context.settings.style.bibliography(entry, previous),
                    context.arena,
                    location,
                )),
                None => push_text(&mut rendered, context.arena, key, location),
            }
            paragraph(rendered, location)
        })
        .collect()
}

fn paragraph<'arena>(content: Vec<InlineNode<'arena>>, location: &Location) -> Block<'arena> {
    Block::Paragraph(Paragraph::new(content, location.clone()))
}

/// Turn formatted spans into inline nodes.
fn spans_to_nodes<'arena>(
    spans: &[Span],
    arena: &'arena DocumentArena,
    location: &Location,
) -> Vec<InlineNode<'arena>> {
    let mut nodes = Vec::with_capacity(spans.len());
    for span in spans {
        match span {
            Span::Text(text) => push_text(&mut nodes, arena, text, location),
            Span::Italic(text) => nodes.push(InlineNode::ItalicText(Italic {
                role: None,
                id: None,
                form: Form::Constrained,
                content: vec![text_node(arena, text, location)],
                location: location.clone(),
            })),
            Span::Link(url) => nodes.push(web_link(arena, url, location)),
        }
    }
    nodes
}

/// A web address, linked to itself.
///
/// A backend turns a bare URL in the source into a link of its own, but the
/// bibliography is built after parsing, so the link has to be built here —
/// otherwise a DOI would arrive as text nobody can follow. An address that
/// will not parse as a URL is left as the text it is.
fn web_link<'arena>(
    arena: &'arena DocumentArena,
    url: &str,
    location: &Location,
) -> InlineNode<'arena> {
    let target = arena.alloc_str(url);
    let Ok(parsed) = SourceUrl::new(target) else {
        return text_node(arena, url, location);
    };
    InlineNode::Macro(InlineMacro::Link(
        Link::new(Source::Url(parsed), location.clone())
            .with_text(vec![text_node(arena, url, location)]),
    ))
}

/// A cross-reference carrying its own text, so it needs no catalog entry.
fn link<'arena>(
    arena: &'arena DocumentArena,
    key: &str,
    label: &str,
    location: &Location,
) -> InlineNode<'arena> {
    let mut xref = CrossReference::new(arena.alloc_str(key), location.clone());
    xref.text = vec![text_node(arena, label, location)];
    InlineNode::Macro(InlineMacro::CrossReference(xref))
}

fn text_node<'arena>(
    arena: &'arena DocumentArena,
    text: &str,
    location: &Location,
) -> InlineNode<'arena> {
    InlineNode::PlainText(Plain {
        content: arena.alloc_str(text),
        location: location.clone(),
        escaped: false,
    })
}

/// The URL schemes a bare address is recognised by.
const URL_SCHEMES: &[&str] = &["https://", "http://", "ftp://", "mailto:"];

/// The first bare web address in `text`: where it starts and where it ends.
///
/// The address has to start a word, and the sentence punctuation that trails
/// it is left outside the link, which is how a backend treats a URL written
/// in the document itself.
fn find_url(text: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(rest) = text.get(from..).filter(|rest| !rest.is_empty()) {
        let (at, scheme) = URL_SCHEMES
            .iter()
            .filter_map(|scheme| rest.find(scheme).map(|at| (at, *scheme)))
            .min_by_key(|(at, _)| *at)?;
        let start = from + at;
        let starts_word = text
            .get(..start)
            .and_then(|before| before.chars().next_back())
            .is_none_or(char::is_whitespace);
        if starts_word {
            let tail = text.get(start..)?;
            let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
            let address = tail
                .get(..end)?
                .trim_end_matches(['.', ',', ';', ':', '!', '?']);
            if address.len() > scheme.len() {
                return Some((start, start + address.len()));
            }
        }
        from = start + scheme.len();
    }
    None
}

/// Append generated text, turning any bare web address in it into a link.
///
/// A backend links a bare URL it finds in the source, but this text is built
/// after parsing, so the split has to happen here — otherwise a URL a `.bib`
/// file put inside a title would arrive as text nobody can follow, while the
/// same URL written in the document would be a link.
fn push_text<'arena>(
    nodes: &mut Vec<InlineNode<'arena>>,
    arena: &'arena DocumentArena,
    text: &str,
    location: &Location,
) {
    let mut rest = text;
    while let Some((start, end)) = find_url(rest) {
        if let Some(before) = rest.get(..start).filter(|before| !before.is_empty()) {
            nodes.push(text_node(arena, before, location));
        }
        if let Some(url) = rest.get(start..end) {
            nodes.push(web_link(arena, url, location));
        }
        let Some(tail) = rest.get(end..) else {
            return;
        };
        rest = tail;
    }
    if !rest.is_empty() {
        nodes.push(text_node(arena, rest, location));
    }
}

/// Put back a stretch of the document's own text, exactly as it was.
///
/// The parser has already decided what this text is — including whether the
/// author escaped something in it — so it is copied rather than looked at
/// again.
fn push_source_text<'arena>(
    nodes: &mut Vec<InlineNode<'arena>>,
    arena: &'arena DocumentArena,
    text: &str,
    source: &Plain<'_>,
) {
    if text.is_empty() {
        return;
    }
    nodes.push(InlineNode::PlainText(Plain {
        content: arena.alloc_str(text),
        location: source.location.clone(),
        escaped: source.escaped,
    }));
}
