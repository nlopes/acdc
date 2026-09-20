//! Reading a BibTeX database.
//!
//! asciidoctor-bibtex delegates this to the `bibtex-ruby` gem. The subset a
//! citation needs is small — entries, their keys, and their fields — so it is
//! read here directly rather than pulling in a parser crate.
//!
//! What is understood: `@type{key, field = value, ...}` entries, values
//! written braced, quoted, or bare, `#` concatenation, and `@string`
//! definitions used through it. `@comment` and `@preamble` are skipped, as is
//! anything outside an entry, which is how BibTeX itself treats stray text.

use std::collections::BTreeMap;

use crate::{error::Error, latex};

/// One entry of a BibTeX database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The citation key, as written.
    pub key: String,
    /// The entry type, lower-cased: `book`, `article`, …
    pub kind: String,
    /// Field values by lower-cased name, with LaTeX markup decoded.
    fields: BTreeMap<String, String>,
}

impl Entry {
    /// A field's value, if the entry has one.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }

    /// The names credited for the work: its authors, or its editors when it
    /// has no authors, which is how a citation falls back for an edited
    /// volume.
    #[must_use]
    pub fn creators(&self) -> Option<&str> {
        self.field("author").or_else(|| self.field("editor"))
    }

    /// Whether the entry credits editors rather than authors.
    #[must_use]
    pub fn is_edited(&self) -> bool {
        self.field("author").is_none() && self.field("editor").is_some()
    }
}

/// A parsed BibTeX database, keyed by citation key.
#[derive(Debug, Default, Clone)]
pub struct Database {
    entries: BTreeMap<String, Entry>,
}

impl Database {
    /// Parse a database from the contents of a `.bib` file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Bibtex`] when an entry is malformed beyond recovery,
    /// such as one whose braces never close.
    pub fn parse(source: &str) -> Result<Self, Error> {
        let mut parser = Parser::new(source);
        let mut entries = BTreeMap::new();
        let mut strings = BTreeMap::new();
        while let Some(item) = parser.next_item()? {
            match item {
                Item::Entry(entry) => {
                    entries.entry(entry.key.clone()).or_insert(entry);
                }
                Item::String { name, value } => {
                    strings.insert(name, value);
                }
                Item::Ignored => {}
            }
        }
        // `@string` definitions are expanded as the entries that use them are
        // read, so a definition has to precede its use — as in BibTeX itself.
        let _ = &strings;
        Ok(Self { entries })
    }

    /// The entry for a citation key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.entries.get(key)
    }
}

/// What one top-level `@…` construct turned out to be.
enum Item {
    Entry(Entry),
    String { name: String, value: String },
    Ignored,
}

struct Parser<'a> {
    input: &'a str,
    position: usize,
    strings: BTreeMap<String, String>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            position: 0,
            strings: BTreeMap::new(),
        }
    }

    /// Read the next `@…` construct, or `None` at the end of the input.
    fn next_item(&mut self) -> Result<Option<Item>, Error> {
        // Anything outside an entry is a comment as far as BibTeX cares.
        let Some(at) = self
            .input
            .get(self.position..)
            .and_then(|rest| rest.find('@'))
        else {
            self.position = self.input.len();
            return Ok(None);
        };
        self.position += at + 1;

        let kind = self
            .take_while(|c| c.is_ascii_alphanumeric())
            .to_ascii_lowercase();
        self.skip_whitespace();
        let Some(open) = self.take_open_delimiter() else {
            // A bare `@` that opens nothing; skip it and carry on.
            return Ok(Some(Item::Ignored));
        };
        let close = closing_for(open);

        if kind == "comment" || kind == "preamble" {
            self.skip_balanced(open, close)?;
            return Ok(Some(Item::Ignored));
        }

        if kind == "string" {
            let (name, value) = self.read_assignment()?;
            self.skip_to_close(close)?;
            self.strings
                .insert(name.to_ascii_lowercase(), value.clone());
            return Ok(Some(Item::String { name, value }));
        }

        self.skip_whitespace();
        let key = self.take_while(|c| c != ',' && c != '}' && c != ')' && !c.is_whitespace());
        let key = key.trim().to_string();
        let mut fields = BTreeMap::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                None => return Err(Error::Bibtex(format!("entry `{key}` is never closed"))),
                Some(c) if c == close => {
                    self.position += c.len_utf8();
                    break;
                }
                Some(',') => {
                    self.position += 1;
                }
                Some(_) => {
                    let (name, value) = self.read_assignment()?;
                    fields.insert(name.to_ascii_lowercase(), latex::decode(&value));
                }
            }
        }

        Ok(Some(Item::Entry(Entry { key, kind, fields })))
    }

    /// Read `name = value`, resolving `#` concatenation and `@string` names.
    fn read_assignment(&mut self) -> Result<(String, String), Error> {
        self.skip_whitespace();
        let name = self
            .take_while(|c| c != '=' && c != ',' && c != '}' && c != ')' && !c.is_whitespace())
            .trim()
            .to_string();
        self.skip_whitespace();
        if self.peek() != Some('=') {
            // A field with no value: BibTeX would reject it, but skipping is
            // friendlier than refusing to read the rest of the database.
            return Ok((name, String::new()));
        }
        self.position += 1;

        let mut value = String::new();
        loop {
            self.skip_whitespace();
            value.push_str(&self.read_value_piece()?);
            self.skip_whitespace();
            if self.peek() == Some('#') {
                self.position += 1;
            } else {
                break;
            }
        }
        Ok((name, value))
    }

    /// One piece of a value: a braced group, a quoted string, or a bare word.
    fn read_value_piece(&mut self) -> Result<String, Error> {
        match self.peek() {
            Some('{') => self.read_braced(),
            Some('"') => self.read_quoted(),
            Some(_) => {
                let word = self
                    .take_while(|c| {
                        c != ',' && c != '#' && c != '}' && c != ')' && !c.is_whitespace()
                    })
                    .to_string();
                // A bare word is either a number or the name of a `@string`.
                Ok(self
                    .strings
                    .get(&word.to_ascii_lowercase())
                    .cloned()
                    .unwrap_or(word))
            }
            None => Err(Error::Bibtex("a field value is missing".to_string())),
        }
    }

    /// Read a `{…}` group, keeping the inner braces: they mark text BibTeX
    /// must not re-case, which the styles still need to see.
    fn read_braced(&mut self) -> Result<String, Error> {
        self.position += 1;
        let start = self.position;
        let mut depth = 1_usize;
        while let Some(c) = self.peek() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let value = self.input.get(start..self.position).unwrap_or_default();
                        self.position += 1;
                        return Ok(value.to_string());
                    }
                }
                '\\' => {
                    // Skip the escaped character so `\}` does not close the group.
                    self.position += c.len_utf8();
                    if let Some(escaped) = self.peek() {
                        self.position += escaped.len_utf8();
                    }
                    continue;
                }
                _ => {}
            }
            self.position += c.len_utf8();
        }
        Err(Error::Bibtex("a braced value is never closed".to_string()))
    }

    fn read_quoted(&mut self) -> Result<String, Error> {
        self.position += 1;
        let start = self.position;
        let mut depth = 0_usize;
        while let Some(c) = self.peek() {
            match c {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                // A quote inside braces is part of the text, not its end.
                '"' if depth == 0 => {
                    let value = self.input.get(start..self.position).unwrap_or_default();
                    self.position += 1;
                    return Ok(value.to_string());
                }
                '\\' => {
                    self.position += c.len_utf8();
                    if let Some(escaped) = self.peek() {
                        self.position += escaped.len_utf8();
                    }
                    continue;
                }
                _ => {}
            }
            self.position += c.len_utf8();
        }
        Err(Error::Bibtex("a quoted value is never closed".to_string()))
    }

    fn take_open_delimiter(&mut self) -> Option<char> {
        match self.peek() {
            Some('{') => {
                self.position += 1;
                Some('{')
            }
            Some('(') => {
                self.position += 1;
                Some('(')
            }
            _ => None,
        }
    }

    fn skip_balanced(&mut self, open: char, close: char) -> Result<(), Error> {
        let mut depth = 1_usize;
        while let Some(c) = self.peek() {
            self.position += c.len_utf8();
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
        }
        Err(Error::Bibtex("a block is never closed".to_string()))
    }

    fn skip_to_close(&mut self, close: char) -> Result<(), Error> {
        while let Some(c) = self.peek() {
            self.position += c.len_utf8();
            if c == close {
                return Ok(());
            }
        }
        Err(Error::Bibtex("a block is never closed".to_string()))
    }

    fn peek(&self) -> Option<char> {
        self.input
            .get(self.position..)
            .and_then(|rest| rest.chars().next())
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.position += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn take_while(&mut self, accept: impl Fn(char) -> bool) -> &'a str {
        let start = self.position;
        while let Some(c) = self.peek() {
            if accept(c) {
                self.position += c.len_utf8();
            } else {
                break;
            }
        }
        self.input.get(start..self.position).unwrap_or_default()
    }
}

fn closing_for(open: char) -> char {
    if open == '(' { ')' } else { '}' }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    const SAMPLE: &str = r"
@book{brown09,
  editor = {J. Brown},
  title = {Book title},
  publisher = {OUP},
  year = {2009}
}

@book{smith10,
  author = {D. Smith},
  title = {Book title},
  address = {Mahwah, NJ},
  publisher = {Lawrence Erlbaum},
  year = {2010}
}
";

    #[test]
    fn reads_entries_and_fields() {
        let database = Database::parse(SAMPLE).expect("parses");
        let smith = database.get("smith10").expect("smith10");
        assert_eq!(smith.kind, "book");
        assert_eq!(smith.field("author"), Some("D. Smith"));
        assert_eq!(smith.field("address"), Some("Mahwah, NJ"));
        assert_eq!(smith.field("missing"), None);
    }

    #[test]
    fn falls_back_from_authors_to_editors() {
        let database = Database::parse(SAMPLE).expect("parses");
        let brown = database.get("brown09").expect("brown09");
        assert_eq!(brown.creators(), Some("J. Brown"));
        assert!(brown.is_edited());
        assert!(!database.get("smith10").expect("smith10").is_edited());
    }

    #[test]
    fn reads_quoted_and_bare_values() {
        let database = Database::parse("@article{a, title = \"Quoted, with comma\", year = 1999}")
            .expect("parses");
        let entry = database.get("a").expect("a");
        assert_eq!(entry.field("title"), Some("Quoted, with comma"));
        assert_eq!(entry.field("year"), Some("1999"));
    }

    #[test]
    fn expands_string_definitions_and_concatenation() {
        let database = Database::parse(
            "@string{acm = {ACM Press}}\n@book{b, publisher = acm # { and friends}}",
        )
        .expect("parses");
        assert_eq!(
            database.get("b").expect("b").field("publisher"),
            Some("ACM Press and friends")
        );
    }

    #[test]
    fn skips_comments_and_stray_text() {
        let database =
            Database::parse("Some preamble prose.\n@comment{ignored}\n@book{c, year = 2000}")
                .expect("parses");
        assert!(database.get("c").is_some());
    }

    #[test]
    fn reports_an_entry_that_never_closes() {
        assert!(Database::parse("@book{a, title = {Open").is_err());
    }

    #[test]
    fn keeps_the_first_of_two_entries_sharing_a_key() {
        let database =
            Database::parse("@book{dup, year = 1900}\n@book{dup, year = 2000}").expect("parses");
        assert_eq!(
            database.get("dup").expect("dup").field("year"),
            Some("1900")
        );
    }
}
