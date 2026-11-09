use crate::escape::{escape_markup, escape_string};

/// A thin builder around the output markup string that centralizes escaping.
/// Converters use this to assemble Typst *body* markup.
pub struct Writer {
    buffer: String,
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    #[must_use]
    pub fn new() -> Self {
        Writer {
            buffer: String::new(),
        }
    }

    /// Append raw markup verbatim (function calls, punctuation you control).
    pub fn write_raw_markup(&mut self, markup: &str) {
        self.buffer.push_str(markup);
    }

    /// Append literal text, escaped for a content block.
    pub fn write_text(&mut self, text: &str, at_line_start: bool) {
        escape_markup(&mut self.buffer, text, at_line_start);
    }

    /// Append a Typst string literal (`"…"`) with the given contents escaped.
    pub fn write_string_literal(&mut self, text: &str) {
        self.buffer.push('"');
        escape_string(&mut self.buffer, text);
        self.buffer.push('"');
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.buffer
    }
}

impl std::fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.write_raw_markup(s);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_text_boundary_escapes_each_line_start() {
        let mut writer = Writer::new();
        writer.write_text("first\n- second", false);

        assert_eq!(writer.into_string(), "first\n\\- second");
    }

    #[test]
    fn public_string_boundary_escapes_controls() {
        let mut writer = Writer::new();
        writer.write_string_literal("line\n\u{7f}");

        assert_eq!(writer.into_string(), "\"line\\n\\u{7f}\"");
    }
}
