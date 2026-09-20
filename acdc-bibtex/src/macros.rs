//! Finding the citation macros in a line of text.
//!
//! `cite:[key]`, `citenp:[key(pages)]` and `bibitem:[key]` reach acdc as plain
//! text: the parser has no registry of extension macros, so an unknown one
//! falls through. They are recognised here, in the same shape the gem's
//! regular expressions describe — a type, optional pretext between the colon
//! and the bracket, and a comma-separated list of keys each with an optional
//! parenthesised locator.

/// Whether a citation reads as a parenthetical or as part of the sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `cite:[…]` — the whole citation is bracketed.
    Parenthetical,
    /// `citenp:[…]` — the names read as part of the sentence and only the
    /// date is bracketed.
    Narrative,
}

/// One key in a citation, with the page or range it points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    /// The citation key.
    pub(crate) key: String,
    /// The locator, without its parentheses. Empty when there is none.
    pub(crate) locator: String,
}

/// A recognised macro and where it sits in the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    /// Byte range of the whole macro within the line.
    pub(crate) span: std::ops::Range<usize>,
    /// What the macro asks for.
    pub(crate) macro_call: Call,
}

/// What a macro asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Call {
    /// A citation.
    Citation {
        /// Parenthetical or narrative.
        kind: Kind,
        /// Text written between the colon and the bracket, shown before the
        /// citation.
        pretext: String,
        /// The keys cited.
        items: Vec<Item>,
    },
    /// A rendered bibliography entry, dropped into the text.
    Bibitem {
        /// The key to render.
        key: String,
    },
}

/// Find every citation macro in `line`, in the order they appear.
pub(crate) fn find_all(line: &str) -> Vec<Found> {
    let mut found = Vec::new();
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < line.len() {
        let Some(offset) = line.get(index..).and_then(|rest| rest.find(['c', 'b'])) else {
            break;
        };
        let start = index + offset;
        // A macro name has to start a word, or `precite:[x]` would be one.
        let preceded_by_word = start
            .checked_sub(1)
            .and_then(|before| bytes.get(before))
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        if preceded_by_word {
            index = start + 1;
            continue;
        }
        match parse_at(line, start) {
            Some(item) => {
                index = item.span.end;
                found.push(item);
            }
            None => index = start + 1,
        }
    }
    found
}

/// Parse a macro that starts at `start`, if one does.
fn parse_at(line: &str, start: usize) -> Option<Found> {
    let rest = line.get(start..)?;
    // `citenp` is tried first: `cite` is a prefix of it, and the longer name
    // is the one that matches.
    if let Some(after) = rest.strip_prefix("citenp:") {
        return parse_citation(line, start, start + "citenp:".len(), after, Kind::Narrative);
    }
    if let Some(after) = rest.strip_prefix("cite:") {
        return parse_citation(
            line,
            start,
            start + "cite:".len(),
            after,
            Kind::Parenthetical,
        );
    }
    if let Some(after) = rest.strip_prefix("bibitem:") {
        let open = after.strip_prefix('[')?;
        let close = open.find(']')?;
        let key = open.get(..close)?;
        if key.is_empty() || key.contains(char::is_whitespace) {
            return None;
        }
        let end = start + "bibitem:[".len() + close + 1;
        return Some(Found {
            span: start..end,
            macro_call: Call::Bibitem {
                key: key.to_string(),
            },
        });
    }
    None
}

/// Parse the pretext and key list of a citation macro.
fn parse_citation(
    line: &str,
    start: usize,
    after_colon: usize,
    rest: &str,
    kind: Kind,
) -> Option<Found> {
    // Everything up to the bracket is pretext, and it cannot itself contain
    // one.
    let open = rest.find('[')?;
    let pretext = rest.get(..open)?;
    let body_start = after_colon + open + 1;
    let body = line.get(body_start..)?;
    let close = body.find(']')?;
    let list = body.get(..close)?;

    let items = parse_items(list);
    if items.is_empty() {
        return None;
    }
    Some(Found {
        span: start..body_start + close + 1,
        macro_call: Call::Citation {
            kind,
            pretext: pretext.to_string(),
            items,
        },
    })
}

/// Split a key list into its items.
fn parse_items(list: &str) -> Vec<Item> {
    let mut items = Vec::new();
    for part in list.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, locator) = match part.split_once('(') {
            Some((key, locator)) => (key, locator.strip_suffix(')').unwrap_or(locator)),
            None => (part, ""),
        };
        let key = key.trim();
        if key.is_empty() || key.contains([' ', '[', ']', ')']) {
            continue;
        }
        items.push(Item {
            key: key.to_string(),
            // A LaTeX-style dash in a page range reads as a plain one here.
            locator: locator.trim().replace("--", "-"),
        });
    }
    items
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

    use super::*;

    /// A citation macro reduced to what a test asserts about it.
    type Citation = (Kind, String, Vec<(String, String)>);

    fn citations(line: &str) -> Vec<Citation> {
        find_all(line)
            .into_iter()
            .filter_map(|found| match found.macro_call {
                Call::Citation {
                    kind,
                    pretext,
                    items,
                } => Some((
                    kind,
                    pretext,
                    items
                        .into_iter()
                        .map(|item| (item.key, item.locator))
                        .collect(),
                )),
                Call::Bibitem { .. } => None,
            })
            .collect()
    }

    #[test]
    fn finds_a_citation_in_a_sentence() {
        let found = citations("some text cite:[author12] more text");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Kind::Parenthetical);
        assert_eq!(found[0].2, [("author12".to_string(), String::new())]);
    }

    #[test]
    fn finds_several_keys_in_one_macro() {
        let found = citations("some text cite:[author12,another11] more");
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].2,
            [
                ("author12".to_string(), String::new()),
                ("another11".to_string(), String::new())
            ]
        );
    }

    #[test]
    fn finds_separate_macros_in_one_line() {
        let found = citations("text cite:[author12,another11] more cite:[third10]");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].2.len(), 2);
        assert_eq!(found[1].2, [("third10".to_string(), String::new())]);
    }

    #[test]
    fn reads_locators() {
        let found = citations("some text citenp:[author12(1-20),another11(15)]");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Kind::Narrative);
        assert_eq!(
            found[0].2,
            [
                ("author12".to_string(), "1-20".to_string()),
                ("another11".to_string(), "15".to_string())
            ]
        );
    }

    #[test]
    fn mixes_keys_with_and_without_locators() {
        let found = citations("citenp:[author12,another11(15-30),third10(14)]");
        assert_eq!(
            found[0].2,
            [
                ("author12".to_string(), String::new()),
                ("another11".to_string(), "15-30".to_string()),
                ("third10".to_string(), "14".to_string())
            ]
        );
    }

    #[test]
    fn accepts_a_dash_in_a_key() {
        let found = citations("cite:[some-author]");
        assert_eq!(found[0].2, [("some-author".to_string(), String::new())]);
    }

    #[test]
    fn reads_the_pretext_before_the_bracket() {
        let found = citations("A bit of pretext: cite:See[Lane12a(89)]");
        assert_eq!(found[0].1, "See");
        assert_eq!(found[0].2, [("Lane12a".to_string(), "89".to_string())]);
    }

    #[test]
    fn normalises_a_latex_dash_in_a_locator() {
        let found = citations("cite:[a(1--20)]");
        assert_eq!(found[0].2, [("a".to_string(), "1-20".to_string())]);
    }

    #[test]
    fn finds_a_bibitem_macro() {
        let found = find_all("- bibitem:[Me2019a]");
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].macro_call,
            Call::Bibitem {
                key: "Me2019a".to_string()
            }
        );
    }

    #[test]
    fn spans_cover_exactly_the_macro() {
        let line = "see cite:[a] here";
        let found = find_all(line);
        assert_eq!(&line[found[0].span.clone()], "cite:[a]");
    }

    #[test]
    fn leaves_text_that_only_looks_like_a_macro() {
        assert!(citations("precite:[a]").is_empty());
        assert!(citations("cite without a bracket").is_empty());
        assert!(citations("cite:[]").is_empty());
        assert!(find_all("bibitem:[]").is_empty());
    }
}
