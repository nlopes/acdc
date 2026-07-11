use serde::Deserialize;

use crate::Error;

/// The colour palette, as CSS-style hexadecimal strings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    /// Page background.
    pub page_bg: String,
    /// Default body text.
    pub body_text: String,
    /// Headings and bold text.
    pub heading: String,
    /// Primary accent, used for the header label.
    pub accent: String,
    /// Hyperlinks.
    pub link: String,
    /// List bullets.
    pub bullet: String,
    /// Ordered-list counters.
    pub counter: String,
    /// Rules, dividers, and table borders.
    pub border: String,
    /// Blockquote text.
    pub quote_text: String,
    /// Blockquote left rule.
    pub quote_rule: String,
    /// Alert/callout card background.
    pub callout_bg: String,
    /// Alert/callout title.
    pub callout_title: String,
    /// Code block background.
    pub code_bg: String,
    /// Code block foreground.
    pub code_fg: String,
}

impl Palette {
    pub(super) fn validate(&self) -> Result<(), Error> {
        for (field, value) in self.entries() {
            canonical_colour(field, value)?;
        }
        Ok(())
    }

    pub(super) fn normalize(&mut self) -> Result<(), Error> {
        for (field, value) in self.entries_mut() {
            *value = canonical_colour(field, value)?;
        }
        Ok(())
    }

    fn entries(&self) -> [(&'static str, &str); 14] {
        [
            ("palette.page_bg", &self.page_bg),
            ("palette.body_text", &self.body_text),
            ("palette.heading", &self.heading),
            ("palette.accent", &self.accent),
            ("palette.link", &self.link),
            ("palette.bullet", &self.bullet),
            ("palette.counter", &self.counter),
            ("palette.border", &self.border),
            ("palette.quote_text", &self.quote_text),
            ("palette.quote_rule", &self.quote_rule),
            ("palette.callout_bg", &self.callout_bg),
            ("palette.callout_title", &self.callout_title),
            ("palette.code_bg", &self.code_bg),
            ("palette.code_fg", &self.code_fg),
        ]
    }

    fn entries_mut(&mut self) -> [(&'static str, &mut String); 14] {
        [
            ("palette.page_bg", &mut self.page_bg),
            ("palette.body_text", &mut self.body_text),
            ("palette.heading", &mut self.heading),
            ("palette.accent", &mut self.accent),
            ("palette.link", &mut self.link),
            ("palette.bullet", &mut self.bullet),
            ("palette.counter", &mut self.counter),
            ("palette.border", &mut self.border),
            ("palette.quote_text", &mut self.quote_text),
            ("palette.quote_rule", &mut self.quote_rule),
            ("palette.callout_bg", &mut self.callout_bg),
            ("palette.callout_title", &mut self.callout_title),
            ("palette.code_bg", &mut self.code_bg),
            ("palette.code_fg", &mut self.code_fg),
        ]
    }
}

fn canonical_colour(field: &'static str, value: &str) -> Result<String, Error> {
    let Some(hex) = value.strip_prefix('#') else {
        return Err(invalid_colour(field));
    };
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_colour(field));
    }

    match hex.len() {
        3 => {
            let mut canonical = String::with_capacity(7);
            canonical.push('#');
            for byte in hex.bytes() {
                let character = char::from(byte).to_ascii_lowercase();
                canonical.push(character);
                canonical.push(character);
            }
            Ok(canonical)
        }
        6 => Ok(value.to_ascii_lowercase()),
        _ => Err(invalid_colour(field)),
    }
}

fn invalid_colour(field: &'static str) -> Error {
    Error::validation(
        field,
        "expected a hexadecimal colour in #rgb or #rrggbb form",
    )
}
