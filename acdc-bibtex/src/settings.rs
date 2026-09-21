//! What the document asks of the bibliography.
//!
//! Every knob is a document attribute, which is how asciidoctor-bibtex is
//! configured; the names and defaults are its.

use acdc_parser::DocumentAttributes;

use crate::style::Style;

/// How the bibliography is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
    /// In the order the document first cites each work.
    Appearance,
    /// By author and year.
    Alphabetical,
}

/// What the citations and the bibliography are rendered as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    /// Formatted text, rendered by whichever backend runs.
    Asciidoc,
    /// `\cite{…}` passthroughs for a LaTeX toolchain to resolve.
    Bibtex,
    /// `\parencite{…}` and `\textcite{…}` passthroughs for biblatex.
    Biblatex,
}

/// What the `bibliography::target[style]` macro asks for.
///
/// The macro's target names the database and its first positional attribute
/// the style, but only as a fallback: a document attribute set in the header
/// or on the command line wins, which is the precedence asciidoctor-bibtex
/// documents.
#[derive(Debug, Clone, Default)]
pub(crate) struct MacroDefaults {
    /// The `.bib` file the macro's target names.
    pub(crate) file: Option<String>,
    /// The style the macro's first attribute names.
    pub(crate) style: Option<String>,
}

/// The document's bibliography settings.
#[derive(Debug, Clone)]
pub(crate) struct Settings {
    /// The `.bib` file, when the document names one.
    pub(crate) file: Option<String>,
    /// The citation style.
    pub(crate) style: Style,
    /// The style name as written, kept for the warning when it is unknown.
    pub(crate) requested_style: Option<String>,
    /// The bibliography order.
    pub(crate) order: Order,
    /// What to render.
    pub(crate) format: Format,
    /// Whether an unknown key is an error rather than a warning.
    pub(crate) throw: bool,
    /// The brackets a numeric citation takes, from `bibtex-citation-template`.
    pub(crate) open: String,
    /// The closing bracket.
    pub(crate) close: String,
}

impl Settings {
    /// Read the settings from a document's attributes, falling back to what
    /// the `bibliography::[]` macro named.
    pub(crate) fn read(attributes: &DocumentAttributes<'_>, defaults: &MacroDefaults) -> Self {
        let text = |name: &str| {
            attributes
                .get(name)
                .and_then(|value| value.text())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };

        let requested_style = text("bibtex-style").or_else(|| defaults.style.clone());
        let style = requested_style
            .as_deref()
            .and_then(Style::parse)
            // The gem's default, and the style its README documents first.
            .unwrap_or(Style::Ieee);

        let (open, close) = text("bibtex-citation-template")
            .as_deref()
            .and_then(split_template)
            .unwrap_or_else(|| ("[".to_string(), "]".to_string()));

        Self {
            file: text("bibtex-file").or_else(|| defaults.file.clone()),
            style,
            requested_style,
            order: match text("bibtex-order").as_deref() {
                Some("alphabetical") => Order::Alphabetical,
                _ => Order::Appearance,
            },
            format: match text("bibtex-format").as_deref() {
                Some("bibtex" | "latex") => Format::Bibtex,
                Some("biblatex") => Format::Biblatex,
                _ => Format::Asciidoc,
            },
            throw: flag(attributes, "bibtex-throw"),
            open,
            close,
        }
    }

    /// Whether the bibliography is listed in citation order.
    ///
    /// A numeric style numbers by position, so appearance order is the only
    /// one that reads sensibly; an author-date style is sorted by author
    /// whatever the attribute says, which is what the gem does.
    pub(crate) fn in_appearance_order(&self) -> bool {
        self.style.is_numeric() && self.order == Order::Appearance
    }
}

/// Whether a yes-or-no attribute is on.
///
/// A bare `:name:` with no value is how `AsciiDoc` turns something on, and
/// the original extension also reads the word `true`; anything else, and an
/// attribute that is unset or absent, is off.
fn flag(attributes: &DocumentAttributes<'_>, name: &str) -> bool {
    attributes.get(name).is_some_and(|value| {
        value.is_presence()
            || value
                .text()
                .is_some_and(|text| text.trim().eq_ignore_ascii_case("true"))
    })
}

/// Split `[$id]` into the text before and after the number.
fn split_template(template: &str) -> Option<(String, String)> {
    let (open, close) = template.split_once("$id")?;
    if open.is_empty() || close.is_empty() {
        return None;
    }
    Some((open.to_string(), close.to_string()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    /// Read the settings from a document whose header holds `lines`.
    ///
    /// The attributes come from parsing a real header rather than being built
    /// by hand, because that is how they reach the pass: a hand-built value
    /// can differ from what the parser makes of the same line.
    fn read_header(lines: &[String], defaults: &MacroDefaults) -> Settings {
        let source = format!("= Doc\n{}\n\nBody.\n", lines.join("\n"));
        let parsed = acdc_parser::parse(&source, &acdc_parser::Options::default()).expect("parse");
        Settings::read(&parsed.document().attributes, defaults)
    }

    fn read(pairs: &[(&str, &str)]) -> Settings {
        let lines: Vec<String> = pairs
            .iter()
            .map(|(name, value)| format!(":{name}: {value}"))
            .collect();
        read_header(&lines, &MacroDefaults::default())
    }

    fn read_with_macro(defaults: &MacroDefaults) -> Settings {
        Settings::read(&DocumentAttributes::default(), defaults)
    }

    #[test]
    fn defaults_match_the_gem() {
        let settings = read(&[]);
        assert_eq!(settings.style, Style::Ieee);
        assert_eq!(settings.order, Order::Appearance);
        assert_eq!(settings.format, Format::Asciidoc);
        assert!(!settings.throw);
        assert_eq!(
            (settings.open.as_str(), settings.close.as_str()),
            ("[", "]")
        );
        assert!(settings.in_appearance_order());
    }

    #[test]
    fn reads_a_yes_or_no_attribute_however_it_is_written() {
        let throws = |line: &str| read_header(&[line.to_string()], &MacroDefaults::default()).throw;
        assert!(throws(":bibtex-throw: true"));
        assert!(throws(":bibtex-throw: TRUE"));
        // A bare entry is how `AsciiDoc` switches something on.
        assert!(throws(":bibtex-throw:"));
        assert!(!throws(":bibtex-throw: false"));
        assert!(!throws(":!bibtex-throw:"));
        assert!(!read(&[]).throw);
    }

    #[test]
    fn reads_each_attribute() {
        let settings = read(&[
            ("bibtex-file", "refs.bib"),
            ("bibtex-style", "apa"),
            ("bibtex-order", "alphabetical"),
            ("bibtex-format", "biblatex"),
            ("bibtex-throw", "true"),
        ]);

        assert_eq!(settings.file.as_deref(), Some("refs.bib"));
        assert_eq!(settings.style, Style::Apa);
        assert_eq!(settings.order, Order::Alphabetical);
        assert_eq!(settings.format, Format::Biblatex);
        assert!(settings.throw);
    }

    #[test]
    fn an_author_date_style_is_never_in_appearance_order() {
        let settings = read(&[("bibtex-style", "apa"), ("bibtex-order", "appearance")]);
        assert!(!settings.in_appearance_order());
    }

    #[test]
    fn reads_a_custom_citation_template() {
        let settings = read(&[("bibtex-citation-template", "/$id/")]);
        assert_eq!(
            (settings.open.as_str(), settings.close.as_str()),
            ("/", "/")
        );
        // A template with nothing on one side is not one.
        let settings = read(&[("bibtex-citation-template", "$id)")]);
        assert_eq!(
            (settings.open.as_str(), settings.close.as_str()),
            ("[", "]")
        );
    }

    #[test]
    fn the_macro_supplies_a_file_and_style_when_no_attribute_does() {
        let settings = read_with_macro(&MacroDefaults {
            file: Some("refs.bib".to_string()),
            style: Some("apa".to_string()),
        });
        assert_eq!(settings.file.as_deref(), Some("refs.bib"));
        assert_eq!(settings.style, Style::Apa);
    }

    #[test]
    fn a_document_attribute_beats_the_macro() {
        let settings = read_header(
            &[
                ":bibtex-file: header.bib".to_string(),
                ":bibtex-style: apa".to_string(),
            ],
            &MacroDefaults {
                file: Some("macro.bib".to_string()),
                style: Some("ieee".to_string()),
            },
        );
        assert_eq!(settings.file.as_deref(), Some("header.bib"));
        assert_eq!(settings.style, Style::Apa);
    }

    #[test]
    fn keeps_an_unknown_style_name_for_reporting() {
        let settings = read(&[("bibtex-style", "nature")]);
        assert_eq!(settings.style, Style::Ieee);
        assert_eq!(settings.requested_style.as_deref(), Some("nature"));
    }
}
