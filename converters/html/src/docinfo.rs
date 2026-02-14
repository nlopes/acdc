use std::path::{Path, PathBuf};

use acdc_parser::{AttributeValue, DocumentAttributes, SafeMode, Substitution, substitute};

/// Resolved docinfo content for each injection position.
///
/// Constructed once per conversion via `DocInfo::resolve()`, then queried
/// at each injection point.
pub(crate) struct DocInfo {
    pub head: Option<String>,
    pub header: Option<String>,
    pub footer: Option<String>,
}

impl DocInfo {
    /// Return an empty `DocInfo` with no content at any position.
    pub fn empty() -> Self {
        Self {
            head: None,
            header: None,
            footer: None,
        }
    }

    /// Resolve all docinfo files for the current document.
    ///
    /// Returns a `DocInfo` with pre-loaded content for each position,
    /// or `None` values when no files exist or docinfo is disabled.
    pub fn resolve(
        attributes: &DocumentAttributes,
        safe_mode: SafeMode,
        source_dir: Option<&Path>,
        docname: Option<&str>,
    ) -> Self {
        // Secure mode disables docinfo entirely
        if safe_mode >= SafeMode::Secure {
            return Self::empty();
        }

        let docinfo_val = match attributes.get("docinfo") {
            Some(AttributeValue::String(s)) if !s.is_empty() => s.clone(),
            // `:docinfo:` set with no value defaults to "private"
            Some(AttributeValue::Bool(true)) => "private".to_string(),
            Some(AttributeValue::String(s)) if s.is_empty() => "private".to_string(),
            _ => return Self::empty(),
        };

        let positions = parse_docinfo_value(&docinfo_val);
        if positions.is_empty() {
            return Self::empty();
        }

        // Resolve docinfodir
        let docinfo_dir = resolve_docinfo_dir(attributes, source_dir);

        // Determine substitutions to apply
        let subs = resolve_docinfo_subs(attributes);

        let head = load_position_content(
            &positions,
            Position::Head,
            &docinfo_dir,
            docname,
            &subs,
            attributes,
        );
        let header = load_position_content(
            &positions,
            Position::Header,
            &docinfo_dir,
            docname,
            &subs,
            attributes,
        );
        let footer = load_position_content(
            &positions,
            Position::Footer,
            &docinfo_dir,
            docname,
            &subs,
            attributes,
        );

        Self {
            head,
            header,
            footer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    Head,
    Header,
    Footer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Shared,
    Private,
}

/// A resolved (scope, position) pair that should be loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EnabledPosition {
    scope: Scope,
    position: Position,
}

/// Parse the `:docinfo:` attribute value into a set of enabled positions.
///
/// Supported values:
/// - `shared` — all 3 shared positions
/// - `private` — all 3 private positions
/// - Granular: `shared-head`, `shared-header`, `shared-footer`,
///   `private-head`, `private-header`, `private-footer`
/// - Comma-separated combinations
fn parse_docinfo_value(value: &str) -> Vec<EnabledPosition> {
    value
        .split(',')
        .flat_map(|token| {
            let token = token.trim();
            match token {
                "shared" => vec![
                    EnabledPosition {
                        scope: Scope::Shared,
                        position: Position::Head,
                    },
                    EnabledPosition {
                        scope: Scope::Shared,
                        position: Position::Header,
                    },
                    EnabledPosition {
                        scope: Scope::Shared,
                        position: Position::Footer,
                    },
                ],
                "private" => vec![
                    EnabledPosition {
                        scope: Scope::Private,
                        position: Position::Head,
                    },
                    EnabledPosition {
                        scope: Scope::Private,
                        position: Position::Header,
                    },
                    EnabledPosition {
                        scope: Scope::Private,
                        position: Position::Footer,
                    },
                ],
                "shared-head" => vec![EnabledPosition {
                    scope: Scope::Shared,
                    position: Position::Head,
                }],
                "shared-header" => vec![EnabledPosition {
                    scope: Scope::Shared,
                    position: Position::Header,
                }],
                "shared-footer" => vec![EnabledPosition {
                    scope: Scope::Shared,
                    position: Position::Footer,
                }],
                "private-head" => vec![EnabledPosition {
                    scope: Scope::Private,
                    position: Position::Head,
                }],
                "private-header" => vec![EnabledPosition {
                    scope: Scope::Private,
                    position: Position::Header,
                }],
                "private-footer" => vec![EnabledPosition {
                    scope: Scope::Private,
                    position: Position::Footer,
                }],
                _ => {
                    tracing::warn!(token, "unknown docinfo value, ignoring");
                    vec![]
                }
            }
        })
        .collect()
}

/// Resolve the directory where docinfo files are located.
///
/// Uses `:docinfodir:` if set (absolute or resolved relative to source dir),
/// otherwise defaults to the source document directory.
fn resolve_docinfo_dir(attributes: &DocumentAttributes, source_dir: Option<&Path>) -> PathBuf {
    let base = source_dir.unwrap_or_else(|| Path::new("."));

    if let Some(AttributeValue::String(dir)) = attributes.get("docinfodir")
        && !dir.is_empty()
    {
        let dir_path = Path::new(dir.as_str());
        if dir_path.is_absolute() {
            return dir_path.to_path_buf();
        }
        return base.join(dir_path);
    }

    base.to_path_buf()
}

/// Determine substitutions to apply to docinfo content.
///
/// `:docinfosubs:` controls this; default is `attributes` only.
fn resolve_docinfo_subs(attributes: &DocumentAttributes) -> Vec<Substitution> {
    if let Some(AttributeValue::String(subs_str)) = attributes.get("docinfosubs") {
        let mut subs = Vec::new();
        for token in subs_str.split(',') {
            match token.trim() {
                "attributes" => subs.push(Substitution::Attributes),
                other => {
                    tracing::warn!(sub = other, "unsupported docinfosubs value, ignoring");
                }
            }
        }
        subs
    } else {
        vec![Substitution::Attributes]
    }
}

/// Build the filename for a docinfo file.
///
/// Shared files: `docinfo.html`, `docinfo-header.html`, `docinfo-footer.html`
/// Private files: `{docname}-docinfo.html`, `{docname}-docinfo-header.html`, etc.
fn docinfo_filename(scope: Scope, position: Position, docname: Option<&str>) -> Option<String> {
    let position_suffix = match position {
        Position::Head => "",
        Position::Header => "-header",
        Position::Footer => "-footer",
    };

    match scope {
        Scope::Shared => Some(format!("docinfo{position_suffix}.html")),
        Scope::Private => {
            let name = docname?;
            Some(format!("{name}-docinfo{position_suffix}.html"))
        }
    }
}

/// Load and concatenate content for a specific position from all enabled scopes.
///
/// Private content comes first, then shared (matching asciidoctor ordering).
fn load_position_content(
    positions: &[EnabledPosition],
    target: Position,
    docinfo_dir: &Path,
    docname: Option<&str>,
    subs: &[Substitution],
    attributes: &DocumentAttributes,
) -> Option<String> {
    let mut parts = Vec::new();

    // Private first, then shared
    for scope in [Scope::Private, Scope::Shared] {
        let enabled = positions
            .iter()
            .any(|p| p.scope == scope && p.position == target);
        if !enabled {
            continue;
        }

        let Some(filename) = docinfo_filename(scope, target, docname) else {
            continue;
        };

        let path = docinfo_dir.join(&filename);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let processed = if subs.is_empty() {
                    content
                } else {
                    substitute(&content, subs, attributes)
                };
                if !processed.trim().is_empty() {
                    parts.push(processed);
                }
            }
            Err(e) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %e,
                    "docinfo file not found or unreadable"
                );
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_shared_expands_all_positions() {
        let positions = parse_docinfo_value("shared");
        assert_eq!(positions.len(), 3);
        assert!(positions.iter().all(|p| p.scope == Scope::Shared));
        assert!(positions.iter().any(|p| p.position == Position::Head));
        assert!(positions.iter().any(|p| p.position == Position::Header));
        assert!(positions.iter().any(|p| p.position == Position::Footer));
    }

    #[test]
    fn parse_private_expands_all_positions() {
        let positions = parse_docinfo_value("private");
        assert_eq!(positions.len(), 3);
        assert!(positions.iter().all(|p| p.scope == Scope::Private));
    }

    #[test]
    fn parse_granular_values() {
        let positions = parse_docinfo_value("shared-head,private-footer");
        assert_eq!(positions.len(), 2);
        assert!(
            positions
                .first()
                .is_some_and(|p| p.scope == Scope::Shared && p.position == Position::Head)
        );
        assert!(
            positions
                .get(1)
                .is_some_and(|p| p.scope == Scope::Private && p.position == Position::Footer)
        );
    }

    #[test]
    fn parse_comma_separated_combination() {
        let positions = parse_docinfo_value("shared, private-footer");
        assert_eq!(positions.len(), 4); // 3 shared + 1 private-footer
    }

    #[test]
    fn parse_unknown_value_ignored() {
        let positions = parse_docinfo_value("bogus");
        assert!(positions.is_empty());
    }

    #[test]
    fn docinfo_filename_shared_head() {
        assert_eq!(
            docinfo_filename(Scope::Shared, Position::Head, None),
            Some("docinfo.html".to_string())
        );
    }

    #[test]
    fn docinfo_filename_shared_header() {
        assert_eq!(
            docinfo_filename(Scope::Shared, Position::Header, None),
            Some("docinfo-header.html".to_string())
        );
    }

    #[test]
    fn docinfo_filename_shared_footer() {
        assert_eq!(
            docinfo_filename(Scope::Shared, Position::Footer, None),
            Some("docinfo-footer.html".to_string())
        );
    }

    #[test]
    fn docinfo_filename_private_head() {
        assert_eq!(
            docinfo_filename(Scope::Private, Position::Head, Some("mydoc")),
            Some("mydoc-docinfo.html".to_string())
        );
    }

    #[test]
    fn docinfo_filename_private_no_docname() {
        assert_eq!(docinfo_filename(Scope::Private, Position::Head, None), None);
    }
}
