//! Turning LaTeX markup in a BibTeX field into plain text.
//!
//! A `.bib` file written for LaTeX spells accented letters as macros and wraps
//! text it does not want re-cased in braces. asciidoctor-bibtex hands this to
//! the `latex-decode` gem; the same job is done here for the constructs that
//! actually appear in bibliographies — accents, the letters that have no
//! accent form, dashes and quotes, and the markup macros — after which the
//! braces are dropped.
//!
//! Anything not recognised keeps its argument and loses the macro, which is
//! what `latex-decode` does and is the behaviour that degrades most quietly:
//! an unknown `\mbox{Smith}` still reads as `Smith`.

/// A macro that stands for a single character on its own, such as `\ss`.
const LETTERS: &[(&str, &str)] = &[
    ("ss", "ß"),
    ("AE", "Æ"),
    ("ae", "æ"),
    ("OE", "Œ"),
    ("oe", "œ"),
    ("AA", "Å"),
    ("aa", "å"),
    ("O", "Ø"),
    ("o", "ø"),
    ("L", "Ł"),
    ("l", "ł"),
    ("i", "ı"),
    ("j", "ȷ"),
    ("dag", "†"),
    ("ddag", "‡"),
    ("pounds", "£"),
    ("copyright", "©"),
    ("ldots", "…"),
    ("dots", "…"),
    ("textendash", "–"),
    ("textemdash", "—"),
    ("textquoteleft", "\u{2018}"),
    ("textquoteright", "\u{2019}"),
    ("textquotedblleft", "\u{201C}"),
    ("textquotedblright", "\u{201D}"),
];

/// An accent macro and the combining mark it applies to the next letter.
const ACCENTS: &[(char, char)] = &[
    ('\'', '\u{0301}'), // acute
    ('`', '\u{0300}'),  // grave
    ('^', '\u{0302}'),  // circumflex
    ('"', '\u{0308}'),  // diaeresis
    ('~', '\u{0303}'),  // tilde
    ('=', '\u{0304}'),  // macron
    ('.', '\u{0307}'),  // dot above
];

/// A named accent macro and its combining mark.
const NAMED_ACCENTS: &[(&str, char)] = &[
    ("c", '\u{0327}'), // cedilla
    ("v", '\u{030C}'), // caron
    ("u", '\u{0306}'), // breve
    ("H", '\u{030B}'), // double acute
    ("r", '\u{030A}'), // ring above
    ("k", '\u{0328}'), // ogonek
    ("d", '\u{0323}'), // dot below
    ("b", '\u{0331}'), // bar below
];

/// Macros whose argument is kept and whose own markup is dropped.
const TRANSPARENT: &[&str] = &[
    "emph",
    "textit",
    "textbf",
    "textsc",
    "textrm",
    "texttt",
    "textsf",
    "mbox",
    "hbox",
    "text",
    "mathrm",
    "textnormal",
    "uppercase",
    "lowercase",
    "MakeUppercase",
    "MakeLowercase",
];

/// Decode the LaTeX markup in a field value.
///
/// A `.bib` file wraps long values over several indented lines, which LaTeX
/// reads as a single space, so the line breaks are folded away first — before
/// any macro is expanded, because a macro such as `\url{…}` puts spaces of
/// its own around what it keeps.
#[must_use]
pub(crate) fn decode(value: &str) -> String {
    let decoded = fold_lines(value);
    let decoded = decode_macros(&decoded);
    let decoded = decode_punctuation(&decoded);
    strip_braces(&decoded)
}

/// Collapse each run of whitespace to the single space LaTeX reads it as.
fn fold_lines(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_space = false;
    for c in value.chars() {
        if c.is_whitespace() {
            in_space = true;
            continue;
        }
        if in_space && !out.is_empty() {
            out.push(' ');
        }
        in_space = false;
        out.push(c);
    }
    if in_space && !out.is_empty() {
        out.push(' ');
    }
    out
}

/// Replace every `\…` macro with the text it stands for.
fn decode_macros(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(&next) = chars.peek() else {
            out.push('\\');
            break;
        };

        // `\'e`, `\"o`: an accent whose name is punctuation binds to the next
        // letter, with or without braces around it.
        if let Some((_, mark)) = ACCENTS.iter().find(|(name, _)| *name == next) {
            chars.next();
            out.push_str(&accented(&mut chars, *mark));
            continue;
        }

        // An escaped character stands for itself.
        if !next.is_ascii_alphabetic() {
            chars.next();
            out.push(next);
            continue;
        }

        let mut name = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() {
                name.push(c);
                chars.next();
            } else {
                break;
            }
        }

        if let Some((_, mark)) = NAMED_ACCENTS.iter().find(|(accent, _)| *accent == name) {
            skip_spaces(&mut chars);
            out.push_str(&accented(&mut chars, *mark));
            continue;
        }
        if let Some((_, letter)) = LETTERS.iter().find(|(macro_name, _)| *macro_name == name) {
            // LaTeX ends a control word at the first non-letter and swallows
            // the whitespace that did it, so `\ss e` is one word and `\ss{}e`
            // is the same word written with an explicit terminator.
            consume_empty_group(&mut chars);
            skip_spaces(&mut chars);
            out.push_str(letter);
            continue;
        }
        if name == "url" {
            skip_spaces(&mut chars);
            let argument = take_group(&mut chars);
            // The gem pads a URL with spaces so it stays a separate word.
            out.push(' ');
            out.push_str(&decode_macros(&argument));
            out.push(' ');
            continue;
        }
        if TRANSPARENT.contains(&name.as_str()) {
            skip_spaces(&mut chars);
            let argument = take_group(&mut chars);
            out.push_str(&decode_macros(&argument));
            continue;
        }

        // An unknown macro: keep its argument, drop the macro itself.
        skip_optional_argument(&mut chars);
        skip_spaces(&mut chars);
        if chars.peek() == Some(&'{') {
            let argument = take_group(&mut chars);
            out.push_str(&decode_macros(&argument));
        }
    }
    out
}

/// Apply `mark` to the letter that follows, braced or not.
fn accented(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, mark: char) -> String {
    skip_spaces(chars);
    let letter = if chars.peek() == Some(&'{') {
        take_group(chars)
    } else {
        chars.next().map(String::from).unwrap_or_default()
    };
    let mut out = decode_macros(&letter);
    // The mark follows the letter it modifies; composing here keeps the result
    // comparable to text typed directly.
    out.push(mark);
    compose(&out)
}

/// Fold a base letter and one combining mark into a single character when
/// Unicode has one.
fn compose(text: &str) -> String {
    let mut chars = text.chars();
    let (Some(base), Some(mark), None) = (chars.next(), chars.next(), chars.next()) else {
        return text.to_string();
    };
    COMPOSED
        .iter()
        .find(|(b, m, _)| *b == base && *m == mark)
        .map_or_else(|| text.to_string(), |(_, _, composed)| composed.to_string())
}

/// The precomposed characters a bibliography actually needs.
const COMPOSED: &[(char, char, char)] = &[
    ('a', '\u{0301}', 'á'),
    ('e', '\u{0301}', 'é'),
    ('i', '\u{0301}', 'í'),
    ('o', '\u{0301}', 'ó'),
    ('u', '\u{0301}', 'ú'),
    ('y', '\u{0301}', 'ý'),
    ('c', '\u{0301}', 'ć'),
    ('n', '\u{0301}', 'ń'),
    ('s', '\u{0301}', 'ś'),
    ('z', '\u{0301}', 'ź'),
    ('A', '\u{0301}', 'Á'),
    ('E', '\u{0301}', 'É'),
    ('I', '\u{0301}', 'Í'),
    ('O', '\u{0301}', 'Ó'),
    ('U', '\u{0301}', 'Ú'),
    ('C', '\u{0301}', 'Ć'),
    ('N', '\u{0301}', 'Ń'),
    ('S', '\u{0301}', 'Ś'),
    ('Z', '\u{0301}', 'Ź'),
    ('a', '\u{0300}', 'à'),
    ('e', '\u{0300}', 'è'),
    ('i', '\u{0300}', 'ì'),
    ('o', '\u{0300}', 'ò'),
    ('u', '\u{0300}', 'ù'),
    ('A', '\u{0300}', 'À'),
    ('E', '\u{0300}', 'È'),
    ('I', '\u{0300}', 'Ì'),
    ('O', '\u{0300}', 'Ò'),
    ('U', '\u{0300}', 'Ù'),
    ('a', '\u{0302}', 'â'),
    ('e', '\u{0302}', 'ê'),
    ('i', '\u{0302}', 'î'),
    ('o', '\u{0302}', 'ô'),
    ('u', '\u{0302}', 'û'),
    ('A', '\u{0302}', 'Â'),
    ('E', '\u{0302}', 'Ê'),
    ('I', '\u{0302}', 'Î'),
    ('O', '\u{0302}', 'Ô'),
    ('U', '\u{0302}', 'Û'),
    ('a', '\u{0308}', 'ä'),
    ('e', '\u{0308}', 'ë'),
    ('i', '\u{0308}', 'ï'),
    ('o', '\u{0308}', 'ö'),
    ('u', '\u{0308}', 'ü'),
    ('y', '\u{0308}', 'ÿ'),
    ('A', '\u{0308}', 'Ä'),
    ('E', '\u{0308}', 'Ë'),
    ('I', '\u{0308}', 'Ï'),
    ('O', '\u{0308}', 'Ö'),
    ('U', '\u{0308}', 'Ü'),
    ('a', '\u{0303}', 'ã'),
    ('n', '\u{0303}', 'ñ'),
    ('o', '\u{0303}', 'õ'),
    ('A', '\u{0303}', 'Ã'),
    ('N', '\u{0303}', 'Ñ'),
    ('O', '\u{0303}', 'Õ'),
    ('a', '\u{0304}', 'ā'),
    ('e', '\u{0304}', 'ē'),
    ('i', '\u{0304}', 'ī'),
    ('o', '\u{0304}', 'ō'),
    ('u', '\u{0304}', 'ū'),
    ('c', '\u{0327}', 'ç'),
    ('C', '\u{0327}', 'Ç'),
    ('s', '\u{0327}', 'ş'),
    ('S', '\u{0327}', 'Ş'),
    ('t', '\u{0327}', 'ţ'),
    ('c', '\u{030C}', 'č'),
    ('s', '\u{030C}', 'š'),
    ('z', '\u{030C}', 'ž'),
    ('r', '\u{030C}', 'ř'),
    ('e', '\u{030C}', 'ě'),
    ('n', '\u{030C}', 'ň'),
    ('C', '\u{030C}', 'Č'),
    ('S', '\u{030C}', 'Š'),
    ('Z', '\u{030C}', 'Ž'),
    ('R', '\u{030C}', 'Ř'),
    ('E', '\u{030C}', 'Ě'),
    ('N', '\u{030C}', 'Ň'),
    ('a', '\u{0306}', 'ă'),
    ('A', '\u{0306}', 'Ă'),
    ('g', '\u{0306}', 'ğ'),
    ('G', '\u{0306}', 'Ğ'),
    ('o', '\u{030B}', 'ő'),
    ('u', '\u{030B}', 'ű'),
    ('O', '\u{030B}', 'Ő'),
    ('U', '\u{030B}', 'Ű'),
    ('a', '\u{030A}', 'å'),
    ('A', '\u{030A}', 'Å'),
    ('u', '\u{030A}', 'ů'),
    ('a', '\u{0328}', 'ą'),
    ('e', '\u{0328}', 'ę'),
    ('A', '\u{0328}', 'Ą'),
    ('E', '\u{0328}', 'Ę'),
    ('z', '\u{0307}', 'ż'),
    ('Z', '\u{0307}', 'Ż'),
    ('e', '\u{0307}', 'ė'),
];

/// Dashes and quotes that LaTeX spells with repeated punctuation.
fn decode_punctuation(value: &str) -> String {
    value
        .replace("---", "—")
        .replace("--", "–")
        .replace("``", "\u{201C}")
        .replace("''", "\u{201D}")
        .replace("!`", "¡")
        .replace("?`", "¿")
}

/// Drop the braces BibTeX uses to protect casing, keeping what they held.
fn strip_braces(value: &str) -> String {
    value.chars().filter(|c| *c != '{' && *c != '}').collect()
}

fn skip_spaces(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars.peek() == Some(&' ') {
        chars.next();
    }
}

/// Consume a `{}` that only terminates a macro name.
fn consume_empty_group(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    let mut lookahead = chars.clone();
    if lookahead.next() == Some('{') && lookahead.next() == Some('}') {
        chars.next();
        chars.next();
    }
}

/// Skip a `[…]` optional argument.
fn skip_optional_argument(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    if chars.peek() != Some(&'[') {
        return;
    }
    for c in chars.by_ref() {
        if c == ']' {
            return;
        }
    }
}

/// Take a `{…}` group's contents, or the next character when there is no
/// group.
fn take_group(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    if chars.peek() != Some(&'{') {
        return chars.next().map(String::from).unwrap_or_default();
    }
    chars.next();
    let mut depth = 1_usize;
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return out;
                }
            }
            '\\' => {
                out.push(c);
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
                continue;
            }
            _ => {}
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_accents_with_and_without_braces() {
        assert_eq!(decode(r"Cristi\'{a}n"), "Cristián");
        assert_eq!(decode(r"Cristi\'an"), "Cristián");
        assert_eq!(decode(r#"G\"odel"#), "Gödel");
        assert_eq!(decode(r"Fran\c{c}ois"), "François");
        assert_eq!(decode(r"Dvo\v{r}\'ak"), "Dvořák");
    }

    #[test]
    fn decodes_letters_that_have_no_accent_form() {
        assert_eq!(decode(r"Stra\ss e"), "Straße");
        assert_eq!(decode(r"\AE sop"), "Æsop");
        assert_eq!(decode(r"Erd\H{o}s"), "Erdős");
    }

    #[test]
    fn keeps_the_argument_of_markup_macros() {
        assert_eq!(decode(r"\emph{Book Title}"), "Book Title");
        assert_eq!(decode(r"\textbf{Bold} and \mbox{boxed}"), "Bold and boxed");
        assert_eq!(decode(r"\unknownmacro{kept}"), "kept");
    }

    #[test]
    fn pads_a_url_so_it_stays_a_word() {
        assert_eq!(
            decode(r"See \url{http://example.com} now"),
            "See  http://example.com  now"
        );
    }

    #[test]
    fn decodes_dashes_and_quotes() {
        assert_eq!(decode("1--20"), "1–20");
        assert_eq!(decode("pages 1---2"), "pages 1—2");
        assert_eq!(decode("``quoted''"), "\u{201C}quoted\u{201D}");
    }

    #[test]
    fn strips_the_braces_that_protect_casing() {
        assert_eq!(decode("The {BibTeX} Manual"), "The BibTeX Manual");
    }

    #[test]
    fn keeps_escaped_characters() {
        assert_eq!(decode(r"Smith \& Jones"), "Smith & Jones");
        assert_eq!(decode(r"100\% sure"), "100% sure");
    }
}
