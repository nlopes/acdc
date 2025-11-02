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
