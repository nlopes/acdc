//! Formatting an entry the way a citation style wants it.
//!
//! asciidoctor-bibtex renders through citeproc and the CSL style files, which
//! between them cover several thousand styles. acdc implements the three the
//! gem's own documentation and tests are written against — `ieee`, `apa` and
//! `chicago-author-date` — directly, because a CSL engine is a project of its
//! own. An unrecognised style name is reported and falls back to `ieee`.
//!
//! The output is a short list of [`Span`]s rather than a markup string, so the
//! caller can turn it into whichever node a backend needs without parsing
//! markup back out of text.

use crate::{
    database::Entry,
    names::{self, Name},
};

/// A run of formatted text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Span {
    /// Plain text.
    Text(String),
    /// Text set in italics: a title, or a journal name.
    Italic(String),
    /// A web address, shown as itself and linked to itself.
    Link(String),
}

/// A citation style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style {
    /// A numeric style: citations are `[1]`, the list is in citation order.
    Ieee,
    /// An author-date style with `&` between names.
    Apa,
    /// An author-date style with `and` between names and title-cased titles.
    ChicagoAuthorDate,
}

/// Every style name that resolves, with the aliases CSL uses for them.
pub(crate) const NAMES: &[&str] = &["ieee", "apa", "chicago-author-date", "chicago", "harvard"];

impl Style {
    /// Resolve a `bibtex-style` value.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "ieee" => Some(Style::Ieee),
            // APA is the Harvard style CSL ships under that name, and the
            // gem's documentation calls it "Harvard-like".
            "apa" | "harvard" => Some(Style::Apa),
            "chicago-author-date" | "chicago" => Some(Style::ChicagoAuthorDate),
            _ => None,
        }
    }

    /// The name this style is written as.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Style::Ieee => "ieee",
            Style::Apa => "apa",
            Style::ChicagoAuthorDate => "chicago-author-date",
        }
    }

    /// Whether citations are numbers rather than names and years.
    pub(crate) fn is_numeric(self) -> bool {
        self == Style::Ieee
    }

    /// What separates two citations inside one macro.
    pub(crate) fn item_separator(self) -> &'static str {
        if self.is_numeric() { "," } else { ";" }
    }

    /// Render a locator — a page or range — the way this style appends it to
    /// a citation.
    pub(crate) fn locator(self, locator: &str) -> String {
        if locator.is_empty() {
            return String::new();
        }
        let mut result = String::new();
        if !self.is_numeric() {
            result.push(',');
        }
        result.push(' ');
        // Chicago cites a bare page number after the year; the others label
        // it. A plain number is one page, and anything else may be a range or
        // a note, which `pp.` covers both of. The space is a non-breaking one
        // so the number never starts a line alone — written as the character
        // rather than a reference, because this becomes a text node that every
        // backend renders, not HTML source.
        if self == Style::ChicagoAuthorDate {
            result.push_str(locator);
        } else {
            let label = if locator.chars().all(|c| c.is_ascii_digit()) {
                "p.\u{a0}"
            } else {
                "pp.\u{a0}"
            };
            result.push_str(label);
            result.push_str(locator);
        }
        result
    }

    /// The citation text for one entry: the names and the year, with no
    /// brackets and no locator.
    ///
    /// A numeric style has no such text — its citations are the entry's
    /// position in the bibliography — so this is only called for the others.
    pub(crate) fn citation(self, entry: &Entry) -> String {
        let names = names::parse_list(entry.creators().unwrap_or_default());
        let year = entry.field("year").unwrap_or_default();
        let families: Vec<String> = names.iter().map(|name| name.family.clone()).collect();
        // APA abbreviates from the third name, Chicago only from the fourth.
        let listed = if self == Style::Apa { 2 } else { 3 };
        let who = match families.as_slice() {
            [] => String::new(),
            [first, ..] if families.len() > listed => format!("{first} et al."),
            _ => {
                let joiner = if self == Style::Apa { "&" } else { "and" };
                names::join(&families, joiner, false)
            }
        };
        if self == Style::Apa {
            format!("{who}, {year}")
        } else {
            format!("{who} {year}")
        }
    }

    /// The bibliography entry.
    ///
    /// `previous` is the entry listed just before this one, which Chicago
    /// needs so that an author credited twice in a row is replaced by a dash.
    pub(crate) fn bibliography(self, entry: &Entry, previous: Option<&Entry>) -> Vec<Span> {
        let mut spans = match self {
            Style::Ieee => ieee(entry),
            Style::Apa => apa(entry),
            Style::ChicagoAuthorDate => chicago(entry, previous),
        };
        trim_end(&mut spans);
        spans
    }
}

/// What kind of work an entry describes.
///
/// BibTeX entry types map many-to-one onto the handful of shapes a style
/// really distinguishes. A type this port does not know is placed by its
/// fields instead, because `.bib` files in the wild invent types freely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// An article in a periodical.
    Article,
    /// A chapter or other part of a larger book.
    Chapter,
    /// A paper in the proceedings of a conference.
    Paper,
    /// A thesis, labelled with the degree it was submitted for.
    Thesis(&'static str),
    /// A report, issued by an institution rather than published.
    Report,
    /// A work that stands on its own, with a publisher.
    Book,
    /// Anything else: a title and a year, and whatever else it carries.
    Other,
}

fn shape(entry: &Entry) -> Shape {
    match entry.kind.as_str() {
        "article" | "periodical" => Shape::Article,
        "incollection" | "inbook" => Shape::Chapter,
        "inproceedings" | "conference" => Shape::Paper,
        "phdthesis" => Shape::Thesis("PhD thesis"),
        "mastersthesis" => Shape::Thesis("Master\u{2019}s thesis"),
        "thesis" => Shape::Thesis("Thesis"),
        // An unpublished piece is a report without an institution: the same
        // shape, and the same punctuation around the title.
        "techreport" | "report" | "unpublished" => Shape::Report,
        "book" | "manual" | "proceedings" | "collection" => Shape::Book,
        // `booklet`, `misc` and `unpublished` fall through with the rest: a
        // work with no publisher behind it is set like any other loose piece.
        _ if entry.field("journal").is_some() => Shape::Article,
        _ if entry.field("booktitle").is_some() => Shape::Chapter,
        _ if entry.field("publisher").is_some() => Shape::Book,
        _ => Shape::Other,
    }
}

/// Whoever stands behind the work: its publisher, or the body that issued it.
fn issuer(entry: &Entry) -> Option<&str> {
    entry
        .field("publisher")
        .or_else(|| entry.field("institution"))
        .or_else(|| entry.field("organization"))
        .or_else(|| entry.field("school"))
}

/// Build the credited names for a style, and the marker an edited work takes.
fn credits(
    entry: &Entry,
    arrange: impl Fn(usize, &Name) -> String,
    joiner: &str,
    serial: bool,
) -> String {
    let names = names::parse_list(entry.creators().unwrap_or_default());
    let parts: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(index, name)| arrange(index, name))
        .collect();
    names::join(&parts, joiner, serial || parts.len() > 2)
}

/// How many editors an entry credits, for the singular/plural marker.
fn editor_count(entry: &Entry) -> usize {
    names::parse_list(entry.creators().unwrap_or_default()).len()
}

fn push_text(spans: &mut Vec<Span>, text: impl Into<String>) {
    let text = text.into();
    if text.is_empty() {
        return;
    }
    match spans.last_mut() {
        Some(Span::Text(existing)) => existing.push_str(&text),
        _ => spans.push(Span::Text(text)),
    }
}

fn push_italic(spans: &mut Vec<Span>, text: &str) {
    if !text.is_empty() {
        spans.push(Span::Italic(text.to_string()));
    }
}

fn push_link(spans: &mut Vec<Span>, url: &str) {
    if !url.is_empty() {
        spans.push(Span::Link(url.to_string()));
    }
}

/// Drop trailing space, which a missing publisher or journal can leave behind.
fn trim_end(spans: &mut Vec<Span>) {
    while let Some(Span::Text(last)) = spans.last_mut() {
        let trimmed = last.trim_end();
        if trimmed.len() == last.len() {
            return;
        }
        if trimmed.is_empty() {
            spans.pop();
        } else {
            last.truncate(trimmed.len());
            return;
        }
    }
}

/// IEEE: initials first, the title quoted unless the work stands on its own,
/// and the year last.
fn ieee(entry: &Entry) -> Vec<Span> {
    let mut spans = Vec::new();
    let who = credits(entry, |_, name| name.initials_first(), "and", false);
    if !who.is_empty() {
        push_text(&mut spans, who);
        if entry.is_edited() {
            push_text(
                &mut spans,
                if editor_count(entry) > 1 {
                    ", Eds."
                } else {
                    ", Ed."
                },
            );
        }
        push_text(&mut spans, ", ");
    }

    let title = entry.field("title").unwrap_or_default();
    let year = entry.field("year").unwrap_or_default();
    let shape = shape(entry);
    match shape {
        Shape::Book => {
            push_italic(&mut spans, title);
            push_text(&mut spans, ". ");
        }
        // A loose work keeps the period inside its quotation marks, because
        // nothing follows the title but the year.
        Shape::Other => push_text(&mut spans, format!("\u{201C}{title}.\u{201D} ")),
        Shape::Article | Shape::Chapter | Shape::Paper | Shape::Thesis(_) | Shape::Report => {
            push_text(&mut spans, format!("\u{201C}{title},\u{201D} "));
        }
    }

    match shape {
        Shape::Article => {
            push_italic(&mut spans, entry.field("journal").unwrap_or_default());
            push_text(&mut spans, ", ");
            if let Some(volume) = entry.field("volume") {
                push_text(&mut spans, format!("vol. {volume}, "));
            }
            if let Some(number) = entry.field("number") {
                push_text(&mut spans, format!("no. {number}, "));
            }
            if let Some(pages) = entry.field("pages") {
                push_text(&mut spans, format!("pp. {pages}, "));
            }
            if let Some(month) = entry.field("month").and_then(month_abbreviation) {
                push_text(&mut spans, format!("{month} "));
            }
            push_text(&mut spans, year);
            if let Some(doi) = entry.field("doi") {
                push_text(&mut spans, format!(", doi: {doi}"));
            }
            push_text(&mut spans, ".");
        }
        Shape::Chapter | Shape::Paper => {
            push_text(&mut spans, "in ");
            push_italic(&mut spans, entry.field("booktitle").unwrap_or_default());
            push_text(&mut spans, ", ");
            push_imprint_ieee(&mut spans, entry);
            push_text(&mut spans, year);
            if let Some(pages) = entry.field("pages") {
                push_text(&mut spans, format!(", pp. {pages}"));
            }
            push_text(&mut spans, ".");
        }
        Shape::Thesis(label) => {
            push_text(&mut spans, format!("{label}, "));
            if let Some(school) = entry.field("school").or_else(|| issuer(entry)) {
                push_text(&mut spans, format!("{school}, "));
            }
            push_text(&mut spans, format!("{year}."));
        }
        Shape::Report => {
            if let Some(issuer) = issuer(entry) {
                push_text(&mut spans, format!("{issuer}, "));
            }
            push_text(&mut spans, format!("{year}."));
        }
        Shape::Book => {
            push_imprint_ieee(&mut spans, entry);
            push_text(&mut spans, format!("{year}."));
        }
        Shape::Other => {
            push_text(&mut spans, year);
            if let Some(url) = entry.field("url") {
                push_text(&mut spans, ", [Online]. Available: ");
                push_link(&mut spans, url);
            }
            push_text(&mut spans, ".");
        }
    }
    spans
}

/// `Place: Publisher, `, the imprint IEEE sets before the year.
fn push_imprint_ieee(spans: &mut Vec<Span>, entry: &Entry) {
    if let Some(address) = entry.field("address") {
        push_text(spans, format!("{address}: "));
    }
    if let Some(issuer) = issuer(entry) {
        push_text(spans, format!("{issuer}, "));
    }
}

/// APA: family names first with initials, the year in parentheses, and the
/// work's address on the web last.
fn apa(entry: &Entry) -> Vec<Span> {
    let mut spans = Vec::new();
    let who = credits(entry, |_, name| name.family_then_initials(), "&", true);
    if !who.is_empty() {
        push_text(&mut spans, who);
        if entry.is_edited() {
            push_text(
                &mut spans,
                if editor_count(entry) > 1 {
                    " (Eds.)."
                } else {
                    " (Ed.)."
                },
            );
        }
        push_text(&mut spans, " ");
    }
    push_text(
        &mut spans,
        format!("({}). ", entry.field("year").unwrap_or_default()),
    );

    let title = entry.field("title").unwrap_or_default();
    let shape = shape(entry);
    match shape {
        // The title of a part of a larger work is plain; a work that stands
        // on its own is italic.
        Shape::Article | Shape::Chapter | Shape::Paper => {
            push_text(&mut spans, format!("{title}. "));
        }
        // A work that stands on its own is italic, and what follows the
        // title supplies its own punctuation.
        Shape::Thesis(_) | Shape::Report | Shape::Book | Shape::Other => {
            push_italic(&mut spans, title);
        }
    }

    match shape {
        Shape::Article => {
            push_italic(&mut spans, entry.field("journal").unwrap_or_default());
            // Without a volume the issue takes its place, which is what a
            // journal numbered only by issue ends up looking like.
            match (entry.field("volume"), entry.field("number")) {
                (Some(volume), number) => {
                    push_text(&mut spans, ", ");
                    push_italic(&mut spans, volume);
                    if let Some(number) = number {
                        push_text(&mut spans, format!("({number})"));
                    }
                }
                (None, Some(number)) => {
                    push_text(&mut spans, ", ");
                    push_italic(&mut spans, number);
                }
                (None, None) => {}
            }
            if let Some(pages) = entry.field("pages") {
                push_text(&mut spans, format!(", {pages}"));
            }
            push_text(&mut spans, ".");
        }
        Shape::Chapter => {
            push_text(&mut spans, "In ");
            push_italic(&mut spans, entry.field("booktitle").unwrap_or_default());
            if let Some(pages) = entry.field("pages") {
                push_text(&mut spans, format!(" (pp. {pages})"));
            }
            push_text(&mut spans, ". ");
            push_issuer_apa(&mut spans, entry);
        }
        Shape::Paper => {
            push_italic(&mut spans, entry.field("booktitle").unwrap_or_default());
            if let Some(pages) = entry.field("pages") {
                push_text(&mut spans, format!(", {pages}"));
            }
            push_text(&mut spans, ".");
        }
        Shape::Thesis(label) => {
            push_text(&mut spans, format!(" [{label}]. "));
            push_issuer_apa(&mut spans, entry);
        }
        Shape::Report | Shape::Book => {
            push_text(&mut spans, ". ");
            push_issuer_apa(&mut spans, entry);
        }
        Shape::Other => push_text(&mut spans, "."),
    }

    // APA closes with the work's address on the web and no full stop after
    // it, so that a copied link does not pick the period up.
    if let Some(link) = web_address(entry) {
        trim_end(&mut spans);
        push_text(&mut spans, " ");
        push_link(&mut spans, &link);
    }
    spans
}

/// The body APA credits with issuing the work, followed by a period.
fn push_issuer_apa(spans: &mut Vec<Span>, entry: &Entry) {
    if let Some(issuer) = issuer(entry) {
        push_text(spans, format!("{issuer}."));
    } else {
        trim_end(spans);
    }
}

/// Chicago author-date: the first name inverted and the rest not, the year
/// after the names, and titles in title case.
fn chicago(entry: &Entry, previous: Option<&Entry>) -> Vec<Span> {
    let mut spans = Vec::new();
    let who = chicago_credits(entry);
    if !who.is_empty() {
        // An author credited again immediately is replaced by a dash, so the
        // eye reads down the list of works rather than re-reading the name.
        let repeated = previous.is_some_and(|previous| chicago_credits(previous) == who);
        let mut credited = if repeated {
            "\u{2014}\u{2014}\u{2014}".to_string()
        } else {
            who
        };
        if entry.is_edited() {
            credited.push_str(if editor_count(entry) > 1 {
                ", eds."
            } else {
                ", ed."
            });
        }
        // The names are a sentence of their own, so they end in a period —
        // unless the last name is an initial that already supplied one.
        if !credited.ends_with('.') {
            credited.push('.');
        }
        push_text(&mut spans, credited);
        push_text(&mut spans, " ");
    }
    push_text(
        &mut spans,
        format!("{}. ", entry.field("year").unwrap_or_default()),
    );

    let title = title_case(entry.field("title").unwrap_or_default());
    let shape = shape(entry);
    if shape == Shape::Book {
        push_italic(&mut spans, &title);
        push_text(&mut spans, ". ");
    } else {
        push_text(&mut spans, format!("\u{201C}{title}.\u{201D} "));
    }

    match shape {
        Shape::Article => {
            push_italic(&mut spans, entry.field("journal").unwrap_or_default());
            let month = entry.field("month").and_then(month_name);
            // The parenthesis after the journal holds the issue, or the month
            // when the work is numbered by date rather than by issue. A
            // journal with neither takes the month after a comma instead.
            let parenthesised = match (entry.field("volume"), entry.field("number")) {
                (Some(volume), number) => {
                    push_text(&mut spans, format!(" {volume}"));
                    number.or(month)
                }
                (None, Some(number)) => {
                    push_text(&mut spans, format!(", no. {number}"));
                    month
                }
                (None, None) => {
                    if let Some(month) = month {
                        push_text(&mut spans, format!(", {month}"));
                    }
                    None
                }
            };
            if let Some(parenthesised) = parenthesised {
                push_text(&mut spans, format!(" ({parenthesised})"));
            }
            if let Some(pages) = entry.field("pages") {
                push_text(&mut spans, format!(": {}", compress_pages(pages)));
            }
            push_text(&mut spans, ".");
        }
        Shape::Chapter | Shape::Paper => {
            push_text(&mut spans, "In ");
            push_italic(
                &mut spans,
                &title_case(entry.field("booktitle").unwrap_or_default()),
            );
            if let Some(pages) = entry.field("pages") {
                push_text(&mut spans, format!(", {pages}"));
            }
            push_text(&mut spans, ". ");
            push_imprint_chicago(&mut spans, entry);
        }
        Shape::Thesis(label) => {
            push_text(&mut spans, format!("{label}, "));
            if let Some(school) = entry.field("school").or_else(|| issuer(entry)) {
                push_text(&mut spans, format!("{school}."));
            }
        }
        Shape::Report | Shape::Book => push_imprint_chicago(&mut spans, entry),
        Shape::Other => {}
    }

    if let Some(link) = web_address(entry) {
        trim_end(&mut spans);
        push_text(&mut spans, " ");
        push_link(&mut spans, &link);
        push_text(&mut spans, ".");
    }
    spans
}

/// The names Chicago files an entry under: the first inverted, the rest not.
fn chicago_credits(entry: &Entry) -> String {
    credits(
        entry,
        |index, name| {
            if index == 0 {
                name.family_first()
            } else {
                name.given_first()
            }
        },
        "and",
        true,
    )
}

/// `Place: Publisher.`, the imprint Chicago sets after the title.
fn push_imprint_chicago(spans: &mut Vec<Span>, entry: &Entry) {
    if let Some(address) = entry.field("address") {
        push_text(spans, format!("{address}: "));
    }
    if let Some(issuer) = issuer(entry) {
        push_text(spans, format!("{issuer}."));
    } else {
        trim_end(spans);
    }
}

/// The twelve months, as a `.bib` file abbreviates them.
const MONTHS: &[&str] = &[
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The month a `month` field names, written in full.
///
/// BibTeX writes a month as one of its own abbreviations (`sep`), which is a
/// bare value rather than a string, but a number or a spelled-out name is
/// common enough in the wild to accept too.
fn month_name(value: &str) -> Option<&'static str> {
    let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if let Ok(number) = value.parse::<usize>() {
        return number
            .checked_sub(1)
            .and_then(|index| MONTHS.get(index))
            .copied();
    }
    let prefix = value.get(..3)?;
    MONTHS
        .iter()
        .find(|name| name.to_ascii_lowercase().starts_with(prefix))
        .copied()
}

/// The month as IEEE abbreviates it: three letters and a period, except for
/// the one month whose name is already that short.
fn month_abbreviation(value: &str) -> Option<String> {
    let name = month_name(value)?;
    if name == "May" {
        return Some(name.to_string());
    }
    name.get(..3).map(|short| format!("{short}."))
}

/// Where the work can be read: its DOI as a resolvable link, or its own URL.
fn web_address(entry: &Entry) -> Option<String> {
    if let Some(doi) = entry.field("doi") {
        return Some(format!("https://doi.org/{doi}"));
    }
    entry.field("url").map(ToString::to_string)
}

/// Words a title case leaves lower-case unless they open or close the title.
const MINOR_WORDS: &[&str] = &[
    "a", "an", "the", "and", "but", "or", "nor", "for", "so", "yet", "as", "at", "by", "in", "of",
    "on", "to", "up", "via", "with", "from", "into", "over", "than", "that", "upon",
];

/// What ends one part of a title and starts the next.
///
/// A hyphen or a slash counts, because the reference implementation
/// capitalises the parts of a hyphenated word separately.
fn is_title_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '-' | '/' | '\u{2013}' | '\u{2014}')
}

/// Capitalise a title the way Chicago sets one.
///
/// Every part is capitalised except the short words a headline leaves alone,
/// and the parts of a hyphenated or slashed word are treated as words of
/// their own. The title's own spacing is left exactly as it was written.
fn title_case(title: &str) -> String {
    // Runs of separators and runs of word characters, alternating, so that
    // the separators can be put back where they were.
    let mut parts: Vec<(bool, String)> = Vec::new();
    for c in title.chars() {
        let is_word = !is_title_separator(c);
        match parts.last_mut() {
            Some((was_word, text)) if *was_word == is_word => text.push(c),
            _ => parts.push((is_word, c.to_string())),
        }
    }

    let words: Vec<usize> = parts
        .iter()
        .enumerate()
        .filter_map(|(index, (is_word, _))| is_word.then_some(index))
        .collect();
    let first = words.first().copied();
    let last = words.last().copied();

    parts
        .iter()
        .enumerate()
        .map(|(index, (is_word, text))| {
            if !is_word {
                return text.clone();
            }
            let lower = text.to_lowercase();
            if Some(index) != first && Some(index) != last && MINOR_WORDS.contains(&lower.as_str())
            {
                lower
            } else {
                capitalise(text)
            }
        })
        .collect()
}

/// Upper-case a word's first letter, leaving a word that is already mixed case
/// — an acronym, a product name — as its author wrote it.
fn capitalise(word: &str) -> String {
    if word.chars().skip(1).any(char::is_uppercase) {
        return word.to_string();
    }
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
    })
}

/// Shorten the second number of a page range, as Chicago does for `1036–60`.
fn compress_pages(pages: &str) -> String {
    let Some((first, last)) = pages
        .split_once('\u{2013}')
        .or_else(|| pages.split_once('-'))
    else {
        return pages.to_string();
    };
    let (first, last) = (first.trim(), last.trim());
    let shortened = shorten_range_end(first, last);
    format!("{first}\u{2013}{shortened}")
}

/// The digits of `last` that differ from `first`, following Chicago 9.61.
///
/// A range below 100, or one that starts on a hundred, keeps every digit.
/// Otherwise the digits shared with `first` are dropped: two remain, or one
/// when the range starts within the first nine of its hundred, so `1036–1060`
/// shortens to `1036–60` and `101–108` to `101–8`. A range that crosses a
/// hundred shares no leading digits and so keeps them all.
fn shorten_range_end<'a>(first: &str, last: &'a str) -> &'a str {
    if first.len() != last.len()
        || first.len() < 3
        || !first.chars().all(|c| c.is_ascii_digit())
        || !last.chars().all(|c| c.is_ascii_digit())
    {
        return last;
    }
    let keep = last.len() - 2;
    if first.get(..keep) != last.get(..keep) {
        return last;
    }
    let Some(within_hundred) = first.get(keep..).and_then(|tail| tail.parse::<u32>().ok()) else {
        return last;
    };
    if within_hundred == 0 {
        return last;
    }
    let tail = last.get(keep..).unwrap_or(last);
    if within_hundred < 10 {
        tail.trim_start_matches('0')
    } else {
        tail
    }
}

/// Render spans the way they would be written in `AsciiDoc`.
///
/// The document pass builds nodes directly and never goes through this; it
/// exists so a formatted entry can be compared against the reference
/// implementation's output in one piece.
#[cfg(test)]
pub(crate) fn to_asciidoc(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|span| match span {
            Span::Text(text) => text.clone(),
            Span::Italic(text) => format!("_{text}_"),
            Span::Link(url) => url.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::database::Database;

    /// The upstream sample database, which the expectations below were taken
    /// from by running `asciidoctor -r asciidoctor-bibtex` over it.
    const SAMPLE: &str = r"
@book{Lane12a, author = {P. Lane}, title = {Book title}, publisher = {Publisher}, year = {2000}}
@book{Lane12b, author = {K. Mane and D. Smith}, title = {Book title}, publisher = {Publisher}, year = {2000}}
@book{Anderson98, editor = {J. R. Anderson and C. Lebiere}, title = {The Atomic Components of Thought},
      publisher = {Lawrence Erlbaum}, address = {Mahwah, NJ}, year = {1998}}
@article{Anderson04, author = {J. R. Anderson and D. Bothell and M. D. Byrne and S. Douglass and C. Lebiere and Y. L. Qin},
      title = {An integrated theory of the mind}, journal = {Psychological Review},
      volume = {111}, number = {4}, pages = {1036--1060}, year = {2004}}
@book{brown09, editor = {J. Brown}, title = {Book title}, publisher = {OUP}, year = {2009}}
@book{smith10, author = {D. Smith}, title = {Book title}, address = {Mahwah, NJ},
      publisher = {Lawrence Erlbaum}, year = {2010}}
";

    fn entry(key: &str) -> Entry {
        Database::parse(SAMPLE)
            .expect("sample parses")
            .get(key)
            .expect("entry is present")
            .clone()
    }

    fn rendered(style: Style, key: &str) -> String {
        to_asciidoc(&style.bibliography(&entry(key), None))
    }

    #[test]
    fn ieee_matches_the_reference() {
        assert_eq!(
            rendered(Style::Ieee, "Lane12a"),
            "P. Lane, _Book title_. Publisher, 2000."
        );
        assert_eq!(
            rendered(Style::Ieee, "Lane12b"),
            "K. Mane and D. Smith, _Book title_. Publisher, 2000."
        );
        assert_eq!(
            rendered(Style::Ieee, "Anderson98"),
            "J. R. Anderson and C. Lebiere, Eds., _The Atomic Components of Thought_. Mahwah, NJ: Lawrence Erlbaum, 1998."
        );
        assert_eq!(
            rendered(Style::Ieee, "Anderson04"),
            "J. R. Anderson, D. Bothell, M. D. Byrne, S. Douglass, C. Lebiere, and Y. L. Qin, \u{201C}An integrated theory of the mind,\u{201D} _Psychological Review_, vol. 111, no. 4, pp. 1036\u{2013}1060, 2004."
        );
        // A single editor takes the singular marker.
        assert_eq!(
            rendered(Style::Ieee, "brown09"),
            "J. Brown, Ed., _Book title_. OUP, 2009."
        );
    }

    #[test]
    fn apa_matches_the_reference() {
        assert_eq!(
            rendered(Style::Apa, "Lane12a"),
            "Lane, P. (2000). _Book title_. Publisher."
        );
        assert_eq!(
            rendered(Style::Apa, "Lane12b"),
            "Mane, K., & Smith, D. (2000). _Book title_. Publisher."
        );
        assert_eq!(
            rendered(Style::Apa, "Anderson98"),
            "Anderson, J. R., & Lebiere, C. (Eds.). (1998). _The Atomic Components of Thought_. Lawrence Erlbaum."
        );
        assert_eq!(
            rendered(Style::Apa, "Anderson04"),
            "Anderson, J. R., Bothell, D., Byrne, M. D., Douglass, S., Lebiere, C., & Qin, Y. L. (2004). An integrated theory of the mind. _Psychological Review_, _111_(4), 1036\u{2013}1060."
        );
        assert_eq!(
            rendered(Style::Apa, "smith10"),
            "Smith, D. (2010). _Book title_. Lawrence Erlbaum."
        );
        assert_eq!(
            rendered(Style::Apa, "brown09"),
            "Brown, J. (Ed.). (2009). _Book title_. OUP."
        );
    }

    #[test]
    fn chicago_matches_the_reference() {
        assert_eq!(
            rendered(Style::ChicagoAuthorDate, "Lane12a"),
            "Lane, P. 2000. _Book Title_. Publisher."
        );
        assert_eq!(
            rendered(Style::ChicagoAuthorDate, "Lane12b"),
            "Mane, K., and D. Smith. 2000. _Book Title_. Publisher."
        );
        assert_eq!(
            rendered(Style::ChicagoAuthorDate, "Anderson98"),
            "Anderson, J. R., and C. Lebiere, eds. 1998. _The Atomic Components of Thought_. Mahwah, NJ: Lawrence Erlbaum."
        );
        assert_eq!(
            rendered(Style::ChicagoAuthorDate, "Anderson04"),
            "Anderson, J. R., D. Bothell, M. D. Byrne, S. Douglass, C. Lebiere, and Y. L. Qin. 2004. \u{201C}An Integrated Theory of the Mind.\u{201D} _Psychological Review_ 111 (4): 1036\u{2013}60."
        );
        assert_eq!(
            rendered(Style::ChicagoAuthorDate, "smith10"),
            "Smith, D. 2010. _Book Title_. Mahwah, NJ: Lawrence Erlbaum."
        );
        assert_eq!(
            rendered(Style::ChicagoAuthorDate, "brown09"),
            "Brown, J., ed. 2009. _Book Title_. OUP."
        );
    }

    #[test]
    fn citations_name_and_date_the_work() {
        assert_eq!(
            Style::ChicagoAuthorDate.citation(&entry("Lane12a")),
            "Lane 2000"
        );
        assert_eq!(Style::Apa.citation(&entry("Lane12a")), "Lane, 2000");
        assert_eq!(
            Style::ChicagoAuthorDate.citation(&entry("Lane12b")),
            "Mane and Smith 2000"
        );
        assert_eq!(Style::Apa.citation(&entry("Lane12b")), "Mane & Smith, 2000");
        // Three or more names are abbreviated.
        assert_eq!(
            Style::ChicagoAuthorDate.citation(&entry("Anderson04")),
            "Anderson et al. 2004"
        );
        assert_eq!(
            Style::Apa.citation(&entry("Anderson04")),
            "Anderson et al., 2004"
        );
        // An edited work is cited by its editors.
        assert_eq!(
            Style::ChicagoAuthorDate.citation(&entry("Anderson98")),
            "Anderson and Lebiere 1998"
        );
    }

    #[test]
    fn locators_follow_the_style() {
        assert_eq!(Style::Ieee.locator("89"), " p.\u{a0}89");
        assert_eq!(Style::Ieee.locator("89-93"), " pp.\u{a0}89-93");
        assert_eq!(Style::Apa.locator("89"), ", p.\u{a0}89");
        assert_eq!(Style::ChicagoAuthorDate.locator("89"), ", 89");
        assert_eq!(Style::ChicagoAuthorDate.locator("89-93"), ", 89-93");
        assert_eq!(Style::Ieee.locator(""), "");
    }

    #[test]
    fn resolves_style_names_and_their_aliases() {
        assert_eq!(Style::parse("ieee"), Some(Style::Ieee));
        assert_eq!(Style::parse("APA"), Some(Style::Apa));
        assert_eq!(Style::parse("chicago"), Some(Style::ChicagoAuthorDate));
        assert_eq!(Style::parse("harvard"), Some(Style::Apa));
        assert_eq!(Style::parse("nature"), None);
        for name in NAMES {
            assert!(
                Style::parse(name).is_some(),
                "`{name}` is listed but does not resolve"
            );
        }
    }

    #[test]
    fn shortens_a_chicago_page_range() {
        assert_eq!(compress_pages("1036\u{2013}1060"), "1036\u{2013}60");
        assert_eq!(compress_pages("101\u{2013}108"), "101\u{2013}8");
        // A hundreds boundary keeps every digit, and so does an uneven range.
        assert_eq!(compress_pages("1000\u{2013}1100"), "1000\u{2013}1100");
        assert_eq!(compress_pages("98\u{2013}104"), "98\u{2013}104");
        assert_eq!(compress_pages("42"), "42");
    }

    #[test]
    fn title_cases_the_way_chicago_does() {
        assert_eq!(
            title_case("An integrated theory of the mind"),
            "An Integrated Theory of the Mind"
        );
        assert_eq!(title_case("Book title"), "Book Title");
        // A word the author capitalised inside stays as written.
        assert_eq!(title_case("The BibTeX manual"), "The BibTeX Manual");
    }
}
