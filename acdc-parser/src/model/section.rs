use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    fmt::Display,
    sync::Arc,
};

use bumpalo::Bump;
use serde::ser::{Serialize, SerializeMap, Serializer};

use crate::{
    Block, BlockMetadata, ColumnStyle, DelimitedBlockType, DocumentAttributes, InlineMacro,
    InlineNode, Location, MAX_SECTION_LEVELS, Reference, Table, TocEntry,
    model::{DocumentAttributeStatus, SectionReference},
};

use super::title::Title;

/// A `SectionLevel` represents a section depth in a document.
pub type SectionLevel = u8;
const DEFAULT_NUMBERED_SECTION_LEVELS: SectionLevel = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SectionNumber(Arc<str>);

impl SectionNumber {
    fn new(number: String) -> Self {
        Self(number.into())
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SectionNumbering {
    enabled: bool,
    max_level: SectionLevel,
    number_all: bool,
    partnums: bool,
    number: Option<SectionNumber>,
}

impl SectionNumbering {
    pub(crate) fn from_attributes(attributes: &DocumentAttributes<'_>) -> Self {
        let numbering = match attributes.status("sectnums") {
            DocumentAttributeStatus::Absent => attributes.status("numbered"),
            status @ (DocumentAttributeStatus::Set(_) | DocumentAttributeStatus::Unset) => status,
        };
        let enabled = matches!(&numbering, DocumentAttributeStatus::Set(_));
        let max_level = attributes
            .text("sectnumlevels")
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_NUMBERED_SECTION_LEVELS)
            .min(MAX_SECTION_LEVELS);
        let number_all = matches!(
            &numbering,
            DocumentAttributeStatus::Set(value) if value.as_str() == Some("all")
        );
        let partnums = attributes.contains_key("partnums");
        Self {
            enabled,
            max_level,
            number_all,
            partnums,
            number: None,
        }
    }

    const fn disabled() -> Self {
        Self {
            enabled: false,
            max_level: DEFAULT_NUMBERED_SECTION_LEVELS,
            number_all: false,
            partnums: false,
            number: None,
        }
    }

    const fn explicitly_enabled() -> Self {
        Self {
            enabled: true,
            max_level: MAX_SECTION_LEVELS,
            number_all: true,
            partnums: true,
            number: None,
        }
    }
}

/// The structural category of a section.
///
/// `AsciiDoc` designates certain section styles as *special sections* — built-in
/// styles for specialized front matter and back matter (preface, glossary, …).
/// `SectionKind` captures that category, derived from the section's style;
/// `Normal` is any ordinary (non-special) section.
///
/// This is a structural classification. Section numbering uses it to apply the
/// special-section rules, and converters use it for presentation.
///
/// `#[non_exhaustive]` so further kinds (e.g. `partintro`, `acknowledgments`)
/// can be added without breaking downstream matches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SectionKind {
    /// An ordinary section (the default).
    #[default]
    Normal,
    Preface,
    Abstract,
    Dedication,
    Colophon,
    Appendix,
    Glossary,
    Bibliography,
    Index,
}

impl SectionKind {
    /// Classify a section from its block style (e.g. `[preface]`). Unknown or
    /// absent styles are `Normal`.
    #[must_use]
    pub fn from_style(style: Option<&str>) -> Self {
        match style {
            Some("preface") => SectionKind::Preface,
            Some("abstract") => SectionKind::Abstract,
            Some("dedication") => SectionKind::Dedication,
            Some("colophon") => SectionKind::Colophon,
            Some("appendix") => SectionKind::Appendix,
            Some("glossary") => SectionKind::Glossary,
            Some("bibliography") => SectionKind::Bibliography,
            Some("index") => SectionKind::Index,
            _ => SectionKind::Normal,
        }
    }

    /// The block style string this kind corresponds to (e.g. `"preface"`), or
    /// `None` for `Normal`. Inverse of [`from_style`](Self::from_style).
    #[must_use]
    pub fn as_style(self) -> Option<&'static str> {
        match self {
            SectionKind::Normal => None,
            SectionKind::Preface => Some("preface"),
            SectionKind::Abstract => Some("abstract"),
            SectionKind::Dedication => Some("dedication"),
            SectionKind::Colophon => Some("colophon"),
            SectionKind::Appendix => Some("appendix"),
            SectionKind::Glossary => Some("glossary"),
            SectionKind::Bibliography => Some("bibliography"),
            SectionKind::Index => Some("index"),
        }
    }

    /// Whether this is a special (front/back-matter) section. True for every
    /// kind except `Normal`. Structural only — implies nothing about rendering.
    #[must_use]
    pub fn is_special(self) -> bool {
        !matches!(self, SectionKind::Normal)
    }
}

/// A `Section` represents a section in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Section<'a> {
    pub metadata: BlockMetadata<'a>,
    pub title: Title<'a>,
    pub level: SectionLevel,
    pub content: Vec<Block<'a>>,
    /// The section's structural category (special-section style, or `Normal`).
    pub kind: SectionKind,
    numbering: SectionNumbering,
    pub(crate) id: Option<&'a str>,
    // Retained until the reference catalog assigns the section's ID.
    pub(crate) reference_text: Option<&'a str>,
    pub location: Location,
}

impl<'a> Section<'a> {
    pub(crate) fn parsed(
        metadata: BlockMetadata<'a>,
        title: Title<'a>,
        level: SectionLevel,
        content: Vec<Block<'a>>,
        kind: SectionKind,
        numbering: SectionNumbering,
        location: Location,
    ) -> Self {
        Self {
            metadata,
            title,
            level,
            content,
            kind,
            numbering,
            id: None,
            reference_text: None,
            location,
        }
    }

    /// Create a new section with the given title, level, content, and location.
    #[must_use]
    pub fn new(
        title: Title<'a>,
        level: SectionLevel,
        content: Vec<Block<'a>>,
        location: Location,
    ) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            title,
            level,
            content,
            kind: SectionKind::Normal,
            numbering: SectionNumbering::disabled(),
            id: None,
            reference_text: None,
            location,
        }
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata<'a>) -> Self {
        self.metadata = metadata;
        self.id = None;
        self
    }

    /// Return the explicit or parser-assigned section ID.
    ///
    /// Parsed sections avoid collisions with preceding targets when generating IDs.
    /// Caller-created sections use the title-derived ID without collision checks.
    #[must_use]
    pub fn id(&self) -> Cow<'a, str> {
        Self::explicit_id(&self.metadata).or(self.id).map_or_else(
            || Cow::Owned(Self::generate_id_string(&self.metadata, &self.title)),
            Cow::Borrowed,
        )
    }

    /// Include or exclude this section from automatic numbering.
    ///
    /// [`Section::new`] disables numbering. Call
    /// [`Document::renumber_sections`](super::Document::renumber_sections) after changing
    /// this policy on a section in a document.
    #[must_use]
    pub fn with_numbering(mut self, enabled: bool) -> Self {
        self.numbering = if enabled {
            SectionNumbering::explicitly_enabled()
        } else {
            SectionNumbering::disabled()
        };
        self
    }

    /// Return the assigned number without presentation punctuation or a signifier.
    ///
    /// Examples are `1`, `1.2`, `IV`, and `A.1`.
    #[must_use]
    pub fn number(&self) -> Option<&str> {
        self.numbering.number.as_ref().map(SectionNumber::as_str)
    }
}

#[derive(Default)]
struct NumberingState {
    counters: [usize; MAX_SECTION_LEVELS as usize + 1],
    part_counter: usize,
    appendix_counter: usize,
    appendix_letter: Option<char>,
    base_level: Option<SectionLevel>,
}

impl NumberingState {
    fn nested_document() -> Self {
        Self {
            base_level: Some(0),
            ..Self::default()
        }
    }

    fn numbering_level(&mut self, source_level: SectionLevel, kind: SectionKind) -> SectionLevel {
        let level = effective_numbering_level(source_level, kind);
        let Some(base) = &mut self.base_level else {
            return level;
        };
        if *base == 0 {
            *base = level;
        }
        level.saturating_sub(*base).saturating_add(1)
    }

    fn next_part(&mut self) -> SectionNumber {
        self.part_counter = self.part_counter.saturating_add(1);
        SectionNumber::new(to_upper_roman(self.part_counter))
    }

    fn next_appendix(&mut self) -> SectionNumber {
        let index = self.appendix_counter;
        self.appendix_counter = self.appendix_counter.saturating_add(1);
        let letter = char::from(b'A' + u8::try_from(index).unwrap_or(25).min(25));
        self.appendix_letter = Some(letter);
        for counter in self.counters.iter_mut().skip(1) {
            *counter = 0;
        }
        SectionNumber::new(letter.to_string())
    }

    fn next_section(&mut self, level: SectionLevel) -> Option<SectionNumber> {
        if level == 0 || level > MAX_SECTION_LEVELS {
            return None;
        }
        if level == 1 {
            self.appendix_letter = None;
        }

        let level_index = usize::from(level - 1);
        let counter = self.counters.get_mut(level_index)?;
        *counter = counter.saturating_add(1);
        for counter in self.counters.iter_mut().skip(level_index + 1) {
            *counter = 0;
        }

        let mut number = String::new();
        if let Some(letter) = self.appendix_letter {
            number.push(letter);
            for component in self.counters.get(1..=level_index)? {
                use std::fmt::Write as _;
                let _ = write!(number, ".{component}");
            }
        } else {
            for (index, component) in self.counters.get(..=level_index)?.iter().enumerate() {
                use std::fmt::Write as _;
                if index > 0 {
                    number.push('.');
                }
                let _ = write!(number, "{component}");
            }
        }
        Some(SectionNumber::new(number))
    }
}

/// Reassign numbers after AST mutation, matching existing TOC entries by section ID.
pub(crate) fn renumber_sections<'a>(
    blocks: &mut [Block<'a>],
    toc_entries: &mut [TocEntry<'a>],
    references: &mut HashMap<&'a str, Reference<'a>>,
    is_book: bool,
) {
    let mut state = NumberingState::default();
    let mut toc_numbers: HashMap<String, VecDeque<Option<SectionNumber>>> =
        HashMap::with_capacity(toc_entries.len());
    let mut record_number =
        |section: &Section<'a>, numbers: SectionNumbers, name, record_toc: bool| {
            update_section_reference(section, name, &numbers, references);
            if record_toc {
                toc_numbers
                    .entry(section.id().into_owned())
                    .or_default()
                    .push_back(numbers.shown);
            }
        };
    renumber_blocks(blocks, &mut state, is_book, false, true, &mut record_number);
    for entry in toc_entries {
        let number = toc_numbers
            .get_mut(entry.id)
            .and_then(VecDeque::pop_front)
            .unwrap_or_default();
        entry.set_number(number);
    }
}

/// The two numbers a section can have.
///
/// They are the same number except past `sectnumlevels`, where Asciidoctor
/// prints the heading with no number but still numbers a cross-reference to
/// it: with `:sectnumlevels: 1`, a level-2 heading reads `Sub part` while
/// `<<sub>>` reads `Section 1.1`. Keeping them apart, and named, is what lets
/// the table of contents follow the heading and the reference catalog follow
/// Asciidoctor.
#[derive(Clone, Debug, Default)]
struct SectionNumbers {
    /// The number printed in the heading and the table of contents.
    shown: Option<SectionNumber>,
    /// The number a cross-reference to the section quotes.
    reference: Option<SectionNumber>,
}

/// The name Asciidoctor gives a section in a cross-reference: the special
/// section's style, or `part`, `chapter` or `section` by level. It selects the
/// `<name>-refsig` attribute and whether a `full` reference emphasises the title.
pub(crate) fn reference_name(kind: SectionKind, level: u8, is_book: bool) -> &'static str {
    kind.as_style().unwrap_or(match level {
        0 => "part",
        1 if is_book => "chapter",
        _ => "section",
    })
}

fn update_section_reference(
    section: &Section<'_>,
    name: &'static str,
    numbers: &SectionNumbers,
    references: &mut HashMap<&str, Reference<'_>>,
) {
    if let Some(reference) = references.get_mut(section.id().as_ref())
        // A duplicate ID must not replace the first target's section metadata.
        && reference.location.absolute_start == section.location.absolute_start
        && reference.location.start == section.location.start
    {
        reference.section = Some(SectionReference {
            name,
            number: numbers.reference.clone(),
        });
    }
}

/// Assign numbers after parsing, matching ordered TOC entries by source location.
pub(crate) fn number_parsed_sections<'a>(
    blocks: &mut [Block<'a>],
    toc_entries: &mut [TocEntry<'a>],
    references: &mut HashMap<&'a str, Reference<'a>>,
    is_book: bool,
) {
    let mut state = NumberingState::default();
    let mut toc_index = 0;
    let mut record_number =
        |section: &Section<'a>, numbers: SectionNumbers, name, record_toc: bool| {
            update_section_reference(section, name, &numbers, references);
            if !record_toc {
                return;
            }
            let Some(remaining_entries) = toc_entries.get(toc_index..) else {
                return;
            };
            let Some(relative_index) = remaining_entries
                .iter()
                .position(|entry| toc_entry_matches(section, entry))
            else {
                return;
            };
            toc_index += relative_index;
            if let Some(entry) = toc_entries.get_mut(toc_index) {
                if let Some(id) = section.id {
                    entry.id = id;
                }
                entry.set_number(numbers.shown);
                toc_index += 1;
            }
        };
    renumber_blocks(blocks, &mut state, is_book, false, true, &mut record_number);
}

fn toc_entry_matches(section: &Section<'_>, entry: &TocEntry<'_>) -> bool {
    entry.level == section.level
        && entry.kind == section.kind
        && entry.title == section.title
        && section.location.absolute_start <= entry.location.absolute_start
        && section.location.absolute_end >= entry.location.absolute_end
}

fn renumber_blocks<'a, F>(
    blocks: &mut [Block<'a>],
    state: &mut NumberingState,
    mut is_book: bool,
    suppressed: bool,
    record_toc: bool,
    record_number: &mut F,
) where
    F: FnMut(&Section<'a>, SectionNumbers, &'static str, bool),
{
    for block in blocks {
        match block {
            Block::Section(section) => {
                renumber_section(
                    section,
                    state,
                    is_book,
                    suppressed,
                    record_toc,
                    record_number,
                );
            }
            Block::Admonition(admonition) => renumber_blocks(
                &mut admonition.blocks,
                state,
                is_book,
                suppressed,
                record_toc,
                record_number,
            ),
            Block::UnorderedList(list) => {
                for item in &mut list.items {
                    renumber_blocks(
                        &mut item.blocks,
                        state,
                        is_book,
                        suppressed,
                        record_toc,
                        record_number,
                    );
                }
            }
            Block::OrderedList(list) => {
                for item in &mut list.items {
                    renumber_blocks(
                        &mut item.blocks,
                        state,
                        is_book,
                        suppressed,
                        record_toc,
                        record_number,
                    );
                }
            }
            Block::CalloutList(list) => {
                for item in &mut list.items {
                    renumber_blocks(
                        &mut item.blocks,
                        state,
                        is_book,
                        suppressed,
                        record_toc,
                        record_number,
                    );
                }
            }
            Block::DescriptionList(list) => {
                for item in &mut list.items {
                    renumber_blocks(
                        &mut item.description,
                        state,
                        is_book,
                        suppressed,
                        record_toc,
                        record_number,
                    );
                }
            }
            Block::DelimitedBlock(delimited) => renumber_delimited(
                &mut delimited.inner,
                state,
                is_book,
                suppressed,
                record_toc,
                record_number,
            ),
            Block::DocumentAttribute(attribute) => {
                if attribute.is_accepted() && attribute.name == "doctype" {
                    is_book = attribute
                        .assignment()
                        .value()
                        .and_then(|value| value.as_str())
                        == Some("book");
                }
            }
            Block::Paragraph(_)
            | Block::DiscreteHeader(_)
            | Block::PageBreak(_)
            | Block::ThematicBreak(_)
            | Block::TableOfContents(_)
            | Block::Image(_)
            | Block::Audio(_)
            | Block::Video(_)
            | Block::Comment(_) => {}
        }
    }
}

fn renumber_delimited<'a, F>(
    inner: &mut DelimitedBlockType<'a>,
    state: &mut NumberingState,
    is_book: bool,
    suppressed: bool,
    record_toc: bool,
    record_number: &mut F,
) where
    F: FnMut(&Section<'a>, SectionNumbers, &'static str, bool),
{
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => renumber_blocks(
            blocks,
            state,
            is_book,
            suppressed,
            record_toc,
            record_number,
        ),
        DelimitedBlockType::DelimitedTable(table) => {
            renumber_table(table, state, is_book, suppressed, record_toc, record_number);
        }
        DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedStem(_) => {}
    }
}

fn renumber_section<'a, F>(
    section: &mut Section<'a>,
    state: &mut NumberingState,
    is_book: bool,
    suppressed: bool,
    record_toc: bool,
    record_number: &mut F,
) where
    F: FnMut(&Section<'a>, SectionNumbers, &'static str, bool),
{
    let level = state.numbering_level(section.level, section.kind);
    let participates = section_participates(section, is_book);
    let eligible = participates && !suppressed;
    let both = |number: SectionNumber| SectionNumbers {
        shown: Some(number.clone()),
        reference: Some(number),
    };
    let numbers = if !eligible {
        SectionNumbers::default()
    } else if section.kind == SectionKind::Appendix {
        both(state.next_appendix())
    } else if section.level == 0 && section.kind == SectionKind::Normal {
        both(state.next_part())
    } else {
        let number = state.next_section(level);
        // Past `sectnumlevels` the heading shows no number, but the counter
        // has advanced and a cross-reference still quotes it; see
        // `SectionNumbers`.
        SectionNumbers {
            shown: (level <= section.numbering.max_level)
                .then(|| number.clone())
                .flatten(),
            reference: number,
        }
    };
    section.numbering.number.clone_from(&numbers.shown);
    record_number(
        section,
        numbers,
        reference_name(section.kind, section.level, is_book),
        record_toc,
    );

    let suppress_children = suppressed
        || (section.kind.is_special() && section.kind != SectionKind::Appendix && !participates);
    renumber_blocks(
        &mut section.content,
        state,
        is_book,
        suppress_children,
        record_toc,
        record_number,
    );
}

fn section_participates(section: &Section<'_>, is_book: bool) -> bool {
    if section.kind == SectionKind::Appendix {
        return true;
    }
    if section.level == 0 && section.kind == SectionKind::Normal {
        return is_book && section.numbering.partnums;
    }
    if !section.numbering.enabled {
        return false;
    }
    match section.kind {
        SectionKind::Normal | SectionKind::Appendix => true,
        SectionKind::Abstract if is_book => true,
        SectionKind::Preface
        | SectionKind::Abstract
        | SectionKind::Dedication
        | SectionKind::Colophon
        | SectionKind::Glossary
        | SectionKind::Bibliography
        | SectionKind::Index => section.numbering.number_all,
    }
}

fn renumber_table<'a, F>(
    table: &mut Table<'a>,
    state: &mut NumberingState,
    is_book: bool,
    suppressed: bool,
    record_toc: bool,
    record_number: &mut F,
) where
    F: FnMut(&Section<'a>, SectionNumbers, &'static str, bool),
{
    for row in table
        .header
        .iter_mut()
        .chain(table.rows.iter_mut())
        .chain(table.footer.iter_mut())
    {
        for column in &mut row.columns {
            if column.style == Some(ColumnStyle::AsciiDoc) {
                let mut nested_state = NumberingState::nested_document();
                renumber_blocks(
                    &mut column.content,
                    &mut nested_state,
                    // AsciiDoc cells start as articles, even inside a book.
                    false,
                    false,
                    false,
                    record_number,
                );
            } else {
                renumber_blocks(
                    &mut column.content,
                    state,
                    is_book,
                    suppressed,
                    record_toc,
                    record_number,
                );
            }
        }
    }
}

fn effective_numbering_level(level: SectionLevel, kind: SectionKind) -> SectionLevel {
    if level == 0 && kind.is_special() {
        1
    } else {
        level
    }
}

fn to_upper_roman(mut number: usize) -> String {
    const NUMERALS: &[(usize, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut result = String::new();
    for &(value, numeral) in NUMERALS {
        while number >= value {
            result.push_str(numeral);
            number -= value;
        }
    }
    result
}

/// A `SafeId` represents a sanitised ID.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SafeId<'a> {
    Generated(&'a str),
    Explicit(&'a str),
}

impl Display for SafeId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // An empty generated slug (a title with no id-able characters)
            // yields an empty id rather than a bare `_`, matching asciidoctor.
            SafeId::Generated("") => Ok(()),
            SafeId::Generated(id) => write!(f, "_{id}"),
            SafeId::Explicit(id) => write!(f, "{id}"),
        }
    }
}

impl<'a> SafeId<'a> {
    /// Return the display-equivalent `&'a str` for this safe id without going
    /// through `format!`/`to_string()`. `Generated` variants are prepended
    /// with `_` into the arena; an empty generated slug stays empty; `Explicit`
    /// is returned unchanged.
    #[must_use]
    pub(crate) fn as_arena_str(&self, arena: &'a Bump) -> &'a str {
        match self {
            SafeId::Generated("") => "",
            SafeId::Generated(id) => {
                let mut s = bumpalo::collections::String::new_in(arena);
                s.push('_');
                s.push_str(id);
                s.into_bump_str()
            }
            SafeId::Explicit(id) => id,
        }
    }
}

impl<'a> Section<'a> {
    /// Build a section id from title text: lowercase, non-alphanumerics
    /// (except whitespace, `-`, `.`, `_`) dropped, survivors joined with `_`,
    /// consecutive `_` collapsed, leading and trailing `_` trimmed. Single
    /// pass. The `_` prefix on a generated id is added by `SafeId`, so leading
    /// separators here must be squeezed away to avoid a doubled `__`.
    fn id_from_title(title: &[InlineNode<'a>]) -> String {
        let mut out = String::new();
        // Start as if a `_` was just emitted so leading separators are dropped.
        let mut last_was_underscore = true;
        Self::append_id_from_inlines(title, &mut out, &mut last_was_underscore);
        while out.ends_with('_') {
            out.pop();
        }
        out
    }

    fn append_id_from_inlines(
        inlines: &[InlineNode<'a>],
        out: &mut String,
        last_was_underscore: &mut bool,
    ) {
        for node in inlines {
            match node {
                InlineNode::PlainText(text) => {
                    Self::append_id_text(text.content, out, last_was_underscore);
                }
                InlineNode::RawText(text) => {
                    Self::append_id_text(text.content, out, last_was_underscore);
                }
                InlineNode::VerbatimText(text) => {
                    Self::append_id_text(text.content, out, last_was_underscore);
                }
                InlineNode::BoldText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::ItalicText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::MonospaceText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::HighlightText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::SubscriptText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::SuperscriptText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::CurvedQuotationText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::CurvedApostropheText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::StandaloneCurvedApostrophe(_) => {
                    Self::append_id_text("'", out, last_was_underscore);
                }
                InlineNode::LineBreak(_) => {
                    Self::append_id_text(" ", out, last_was_underscore);
                }
                InlineNode::InlineAnchor(_) => {}
                InlineNode::Macro(macro_node) => {
                    Self::append_id_from_macro(macro_node, out, last_was_underscore);
                }
                InlineNode::CalloutRef(callout) => {
                    Self::append_id_text(
                        &format!("<{}>", callout.number),
                        out,
                        last_was_underscore,
                    );
                }
            }
        }
    }

    fn append_id_from_macro(
        macro_node: &InlineMacro<'a>,
        out: &mut String,
        last_was_underscore: &mut bool,
    ) {
        match macro_node {
            InlineMacro::Link(link) => {
                if link.text.is_empty() {
                    Self::append_id_text(&link.target.to_string(), out, last_was_underscore);
                } else {
                    Self::append_id_from_inlines(&link.text, out, last_was_underscore);
                }
            }
            InlineMacro::Url(url) => {
                if url.text.is_empty() {
                    Self::append_id_text(&url.target.to_string(), out, last_was_underscore);
                } else {
                    Self::append_id_from_inlines(&url.text, out, last_was_underscore);
                }
            }
            InlineMacro::Mailto(mailto) => {
                if mailto.text.is_empty() {
                    Self::append_id_text(&mailto.target.to_string(), out, last_was_underscore);
                } else {
                    Self::append_id_from_inlines(&mailto.text, out, last_was_underscore);
                }
            }
            InlineMacro::Autolink(autolink) => {
                Self::append_id_text(&autolink.url.to_string(), out, last_was_underscore);
            }
            InlineMacro::CrossReference(xref) => {
                if xref.text.is_empty() {
                    Self::append_id_text(xref.target, out, last_was_underscore);
                } else {
                    Self::append_id_from_inlines(&xref.text, out, last_was_underscore);
                }
            }
            InlineMacro::IndexTerm(index_term) if index_term.is_visible() => {
                Self::append_id_from_inlines(index_term.term(), out, last_was_underscore);
            }
            InlineMacro::Pass(pass) => {
                Self::append_id_text(pass.text.unwrap_or_default(), out, last_was_underscore);
            }
            InlineMacro::Keyboard(keyboard) => {
                Self::append_id_text(&keyboard.keys.join("+"), out, last_was_underscore);
            }
            InlineMacro::Button(button) => {
                Self::append_id_text(button.label, out, last_was_underscore);
            }
            InlineMacro::Menu(menu) => {
                Self::append_id_text(&menu.items.join(" > "), out, last_was_underscore);
            }
            InlineMacro::Image(_)
            | InlineMacro::Footnote(_)
            | InlineMacro::Stem(_)
            | InlineMacro::Icon(_)
            | InlineMacro::IndexTerm(_) => {}
        }
    }

    fn append_id_text(text: &str, out: &mut String, last_was_underscore: &mut bool) {
        for c in text.chars() {
            for c in c.to_lowercase() {
                let mapped = if c.is_alphanumeric() {
                    Some(c)
                } else if c.is_whitespace() || c == '-' || c == '.' || c == '_' {
                    Some('_')
                } else {
                    None
                };
                let Some(ch) = mapped else { continue };
                if ch == '_' {
                    if !*last_was_underscore {
                        out.push('_');
                    }
                    *last_was_underscore = true;
                } else {
                    out.push(ch);
                    *last_was_underscore = false;
                }
            }
        }
    }

    /// Pick the explicit id if metadata provides one, else None. Shared by
    /// the arena-returning and `String`-returning variants below.
    pub(crate) fn explicit_id(metadata: &BlockMetadata<'a>) -> Option<&'a str> {
        if let Some(anchor) = &metadata.id {
            return Some(anchor.id);
        }
        metadata.anchors.last().map(|a| a.id)
    }

    /// Generate a section ID based on its title and metadata.
    ///
    /// Checks in order: explicit `metadata.id` (e.g. `[id=foo]`), then the last entry in
    /// `metadata.anchors` (e.g. `[[foo]]`), otherwise auto-generates one from the title
    /// and interns it into the supplied arena.
    #[must_use]
    pub(crate) fn generate_id(
        arena: &'a Bump,
        metadata: &BlockMetadata<'a>,
        title: &[InlineNode<'a>],
    ) -> SafeId<'a> {
        match Self::explicit_id(metadata) {
            Some(id) => SafeId::Explicit(id),
            None => SafeId::Generated(arena.alloc_str(&Self::id_from_title(title))),
        }
    }

    /// Generate a section ID based on its title and metadata, returning a `String`
    /// directly.
    ///
    /// Returns the `Display`-formatted form (prefixed with `_` for generated IDs)
    /// without checking for collisions. Use [`Section::id`] for parsed sections.
    #[must_use]
    pub fn generate_id_string(metadata: &BlockMetadata<'a>, title: &[InlineNode<'a>]) -> String {
        if let Some(id) = Self::explicit_id(metadata) {
            return id.to_string();
        }
        let slug = Self::id_from_title(title);
        if slug.is_empty() {
            slug
        } else {
            format!("_{slug}")
        }
    }
}

impl Serialize for Section<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "section")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("title", &self.title)?;
        state.serialize_entry("level", &self.level)?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.content.is_empty() {
            state.serialize_entry("blocks", &self.content)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Anchor, Document, Options, Plain, parse};

    use super::*;

    #[test]
    fn nested_section_references_follow_ast_edits_without_a_toc()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse(
            include_str!("../../fixtures/tests/xref_nested_sections.adoc"),
            &Options::default(),
        )?;
        let mut document = Document {
            attributes: parsed.document().attributes.clone(),
            blocks: parsed.document().blocks.clone(),
            references: parsed.document().references.clone(),
            ..Document::default()
        };
        let outer = document
            .blocks
            .iter_mut()
            .find_map(|block| {
                if let Block::Section(section) = block {
                    (section.id() == "outer").then_some(section)
                } else {
                    None
                }
            })
            .ok_or("missing outer section")?;
        let table = outer
            .content
            .iter_mut()
            .find_map(|block| {
                if let Block::DelimitedBlock(delimited) = block
                    && let DelimitedBlockType::DelimitedTable(table) = &mut delimited.inner
                {
                    Some(table)
                } else {
                    None
                }
            })
            .ok_or("missing table")?;
        let cell = table
            .rows
            .first_mut()
            .and_then(|row| row.columns.first_mut())
            .ok_or("missing cell")?;
        let inner = cell
            .content
            .iter()
            .position(|block| matches!(block, Block::Section(section) if section.id() == "inner"))
            .ok_or("missing inner section")?;
        let second = cell
            .content
            .iter()
            .position(|block| matches!(block, Block::Section(section) if section.id() == "second"))
            .ok_or("missing second section")?;
        cell.content.swap(inner, second);

        // Fixtures cannot exercise AST edits or the nonserialized reference catalog.
        document.renumber_sections();
        assert!(document.toc_entries.is_empty());
        for (id, name, number) in [
            ("outer", "chapter", "1"),
            ("inner", "section", "2"),
            ("deep", "section", "2.1"),
            ("second", "section", "1"),
            ("sibling", "section", "1"),
            ("after", "chapter", "2"),
        ] {
            let reference = document.references.get(id).ok_or("missing reference")?;
            assert_eq!(reference.section_name(), Some(name), "{id}");
            assert_eq!(reference.section_number(), Some(number), "{id}");
        }
        Ok(())
    }

    #[test]
    fn nested_duplicate_references_keep_the_first_target_when_renumbered()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse(
            include_str!("../../fixtures/tests/xref_nested_duplicate_ids.adoc"),
            &Options::default(),
        )?;
        let mut document = Document {
            attributes: parsed.document().attributes.clone(),
            blocks: parsed.document().blocks.clone(),
            toc_entries: parsed.document().toc_entries.clone(),
            references: parsed.document().references.clone(),
            ..Document::default()
        };
        document.renumber_sections();
        for (id, reference) in &document.references {
            let original = parsed
                .document()
                .references
                .get(id)
                .ok_or("missing reference")?;
            assert_eq!(reference.section_name(), original.section_name(), "{id}");
            assert_eq!(
                reference.section_number(),
                original.section_number(),
                "{id}"
            );
        }
        Ok(())
    }

    #[test]
    fn test_id_from_title() {
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a title.",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "this_is_a_title".to_string()
        );
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a----title.",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "this_is_a_title".to_string()
        );
    }

    #[test]
    fn test_id_from_title_strips_leading_separators() {
        // Leading non-alphanumerics must be squeezed away so the `_` prefix
        // added by `SafeId::Generated` does not produce a doubled `__`.
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "--  Specialized Environments",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "specialized_environments".to_string()
        );
        assert_eq!(
            Section::generate_id_string(&BlockMetadata::default(), inlines),
            "_specialized_environments".to_string()
        );
    }

    #[test]
    fn test_id_from_title_all_separators_is_empty() {
        // A title with no id-able characters yields an empty id (not a bare
        // `_`), matching asciidoctor.
        let arena = Bump::new();
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "---",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(Section::id_from_title(inlines), String::new());
        let metadata = BlockMetadata::default();
        assert_eq!(
            Section::generate_id_string(&metadata, inlines),
            String::new()
        );
        let safe_id = Section::generate_id(&arena, &metadata, inlines);
        assert_eq!(safe_id, SafeId::Generated(""));
        assert_eq!(safe_id.to_string(), String::new());
        assert_eq!(safe_id.as_arena_str(&arena), "");
    }

    #[test]
    fn test_id_from_title_preserves_underscores() {
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "CHART_BOT",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(Section::id_from_title(inlines), "chart_bot".to_string());
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "haiku_robot",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(Section::id_from_title(inlines), "haiku_robot".to_string());
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "meme_transcriber",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "meme_transcriber".to_string()
        );
    }

    #[test]
    fn test_section_generate_id() {
        let arena = Bump::new();
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a b__i__g title.",
            location: Location::default(),
            escaped: false,
        })];
        // metadata has an empty id
        let metadata = BlockMetadata::default();
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Generated("this_is_a_b_i_g_title")
        );

        // metadata has a specific id in metadata.id
        let metadata = BlockMetadata {
            id: Some(Anchor {
                id: "custom_id",
                xreflabel: None,
                location: Location::default(),
                bibliography: false,
                bibliography_label: None,
            }),
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Explicit("custom_id")
        );

        // metadata has anchor in metadata.anchors (from [[id]] or [#id] syntax)
        let metadata = BlockMetadata {
            anchors: vec![Anchor {
                id: "anchor_id",
                xreflabel: None,
                location: Location::default(),
                bibliography: false,
                bibliography_label: None,
            }],
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Explicit("anchor_id")
        );

        // with multiple anchors, the last one is used (matches asciidoctor behavior)
        let metadata = BlockMetadata {
            anchors: vec![
                Anchor {
                    id: "first_anchor",
                    xreflabel: None,
                    location: Location::default(),
                    bibliography: false,
                    bibliography_label: None,
                },
                Anchor {
                    id: "last_anchor",
                    xreflabel: None,
                    location: Location::default(),
                    bibliography: false,
                    bibliography_label: None,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Explicit("last_anchor")
        );

        // metadata.id takes precedence over metadata.anchors
        let metadata = BlockMetadata {
            id: Some(Anchor {
                id: "from_id",
                xreflabel: None,
                location: Location::default(),
                bibliography: false,
                bibliography_label: None,
            }),
            anchors: vec![Anchor {
                id: "from_anchors",
                xreflabel: None,
                location: Location::default(),
                bibliography: false,
                bibliography_label: None,
            }],
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Explicit("from_id")
        );
    }
}
