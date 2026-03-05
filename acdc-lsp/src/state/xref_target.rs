//! Cross-file xref target parsing

/// Parsed xref target with optional file and anchor components
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XrefTarget {
    /// File path (None = same document)
    pub file: Option<String>,
    /// Anchor ID (None = link to file root)
    pub anchor: Option<String>,
}

impl XrefTarget {
    /// Parse a raw xref target string into file and anchor components.
    ///
    /// Examples:
    /// - `"section-id"` → local anchor
    /// - `"file.adoc#anchor"` → cross-file with anchor
    /// - `"file.adoc"` → cross-file, no anchor
    /// - `"#anchor"` → explicit local anchor
    /// - `"path/to/file.adoc#anchor"` → cross-file with path
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        if let Some(hash_pos) = raw.find('#') {
            let file_part = &raw[..hash_pos];
            let anchor_part = &raw[hash_pos + 1..];

            let file = if file_part.is_empty() {
                None
            } else {
                Some(file_part.to_string())
            };

            let anchor = if anchor_part.is_empty() {
                None
            } else {
                Some(anchor_part.to_string())
            };

            Self { file, anchor }
        } else if std::path::Path::new(raw).extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("adoc") || ext.eq_ignore_ascii_case("asciidoc")
        }) || raw.contains('/')
            || raw.contains('\\')
        {
            // File reference without anchor
            Self {
                file: Some(raw.to_string()),
                anchor: None,
            }
        } else {
            // Local anchor reference
            Self {
                file: None,
                anchor: Some(raw.to_string()),
            }
        }
    }

    /// Returns true if this is a cross-file reference
    #[must_use]
    pub fn is_cross_file(&self) -> bool {
        self.file.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_anchor() {
        let target = XrefTarget::parse("section-id");
        assert_eq!(
            target,
            XrefTarget {
                file: None,
                anchor: Some("section-id".to_string()),
            }
        );
        assert!(!target.is_cross_file());
    }

    #[test]
    fn test_cross_file_with_anchor() {
        let target = XrefTarget::parse("chapter.adoc#my-section");
        assert_eq!(
            target,
            XrefTarget {
                file: Some("chapter.adoc".to_string()),
                anchor: Some("my-section".to_string()),
            }
        );
        assert!(target.is_cross_file());
    }

    #[test]
    fn test_cross_file_without_anchor() {
        let target = XrefTarget::parse("chapter.adoc");
        assert_eq!(
            target,
            XrefTarget {
                file: Some("chapter.adoc".to_string()),
                anchor: None,
            }
        );
    }

    #[test]
    fn test_explicit_local_anchor() {
        let target = XrefTarget::parse("#my-anchor");
        assert_eq!(
            target,
            XrefTarget {
                file: None,
                anchor: Some("my-anchor".to_string()),
            }
        );
    }

    #[test]
    fn test_cross_file_with_path() {
        let target = XrefTarget::parse("docs/chapter.adoc#intro");
        assert_eq!(
            target,
            XrefTarget {
                file: Some("docs/chapter.adoc".to_string()),
                anchor: Some("intro".to_string()),
            }
        );
    }

    #[test]
    fn test_asciidoc_extension() {
        let target = XrefTarget::parse("file.asciidoc");
        assert_eq!(
            target,
            XrefTarget {
                file: Some("file.asciidoc".to_string()),
                anchor: None,
            }
        );
    }

    #[test]
    fn test_path_with_slash() {
        let target = XrefTarget::parse("docs/guide");
        assert_eq!(
            target,
            XrefTarget {
                file: Some("docs/guide".to_string()),
                anchor: None,
            }
        );
    }

    #[test]
    fn test_hash_only_file() {
        let target = XrefTarget::parse("file.adoc#");
        assert_eq!(
            target,
            XrefTarget {
                file: Some("file.adoc".to_string()),
                anchor: None,
            }
        );
    }
}
