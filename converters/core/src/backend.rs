//! Output format backend types.
//!
//! Defines the available converter backends (HTML, manpage, terminal).

use std::str::FromStr;

/// Output format backend type.
///
/// Used by converters to identify themselves and by the CLI for backend selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Backend {
    /// HTML output format.
    #[default]
    Html,
    /// Semantic HTML5 output format (html5s).
    Html5s,
    /// Unix manpage (roff/troff) output format.
    Manpage,
    /// Terminal/console output with ANSI formatting.
    Terminal,
}

impl FromStr for Backend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "html" => Ok(Self::Html),
            "html5s" => Ok(Self::Html5s),
            "manpage" => Ok(Self::Manpage),
            "terminal" => Ok(Self::Terminal),
            _ => Err(format!(
                "invalid backend: '{s}', expected: html, html5s, manpage, terminal"
            )),
        }
    }
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Html => write!(f, "html"),
            Self::Html5s => write!(f, "html5s"),
            Self::Manpage => write!(f, "manpage"),
            Self::Terminal => write!(f, "terminal"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str() {
        assert_eq!(Backend::from_str("html").unwrap(), Backend::Html);
        assert_eq!(Backend::from_str("HTML").unwrap(), Backend::Html);
        assert_eq!(Backend::from_str("html5s").unwrap(), Backend::Html5s);
        assert_eq!(Backend::from_str("HTML5S").unwrap(), Backend::Html5s);
        assert_eq!(Backend::from_str("manpage").unwrap(), Backend::Manpage);
        assert_eq!(Backend::from_str("terminal").unwrap(), Backend::Terminal);
        assert!(Backend::from_str("invalid").is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(Backend::Html.to_string(), "html");
        assert_eq!(Backend::Html5s.to_string(), "html5s");
        assert_eq!(Backend::Manpage.to_string(), "manpage");
        assert_eq!(Backend::Terminal.to_string(), "terminal");
    }

    #[test]
    fn test_default() {
        assert_eq!(Backend::default(), Backend::Html);
    }
}
