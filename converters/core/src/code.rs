use acdc_parser::BlockMetadata;

/// Programming languages for syntax highlighting detection.
const LANGUAGES: &[&str] = &[
    "bash",
    "shell",
    "sh",
    "zsh",
    "fish",
    "python",
    "py",
    "ruby",
    "rb",
    "javascript",
    "js",
    "typescript",
    "ts",
    "java",
    "c",
    "cpp",
    "c++",
    "csharp",
    "cs",
    "go",
    "rust",
    "rs",
    "php",
    "perl",
    "lua",
    "swift",
    "kotlin",
    "scala",
    "clojure",
    "html",
    "xml",
    "css",
    "json",
    "yaml",
    "yml",
    "toml",
    "ini",
    "sql",
    "dockerfile",
    "makefile",
    "cmake",
    "groovy",
];

/// Detect programming language from block metadata for syntax highlighting.
///
/// Returns the language if:
/// - The block has `style="source"`
/// - The metadata contains a recognized language in its attributes
#[must_use]
pub fn detect_language(metadata: &BlockMetadata) -> Option<&str> {
    let is_source = metadata.style.as_deref() == Some("source");
    if !is_source {
        return None;
    }

    // Look for a known language in the attributes
    metadata.attributes.iter().find_map(|(key, _)| {
        if LANGUAGES.contains(&key.as_str()) {
            Some(key.as_str())
        } else {
            None
        }
    })
}

/// Get the default line comment prefix for a programming language.
/// Used for stripping comment guards from callout markers in source blocks.
#[must_use]
pub fn default_line_comment(language: Option<&str>) -> Option<&'static str> {
    match language {
        // Hash comments
        Some(
            "python" | "py" | "ruby" | "rb" | "perl" | "bash" | "shell" | "sh" | "zsh" | "fish"
            | "yaml" | "yml" | "toml" | "dockerfile" | "makefile" | "cmake",
        ) => Some("#"),
        // Double-dash comments (SQL, Lua)
        Some("sql" | "lua") => Some("--"),
        // Semicolon comments
        Some("clojure" | "ini") => Some(";"),
        // XML/HTML comments are multiline, so we return None
        Some("html" | "xml" | "css" | "json") => None,
        // Default: assume C-style (//) for unknown languages and common C-family languages
        _ => Some("//"),
    }
}
