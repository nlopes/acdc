use std::path::PathBuf;

use clap::ValueEnum;

/// document type to use when converting document
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum Doctype {
    #[default]
    Article,
    Book,
    Manpage,
    Inline,
}

impl std::fmt::Display for Doctype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Doctype::Article => write!(f, "article"),
            Doctype::Book => write!(f, "book"),
            Doctype::Manpage => write!(f, "manpage"),
            Doctype::Inline => write!(f, "inline"),
        }
    }
}

/// safe mode to use when converting document
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum SafeMode {
    Safe,
    #[default]
    Unsafe,
    Server,
    Secure,
}

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub generator_metadata: GeneratorMetadata,
    pub doctype: Doctype,
    pub safe_mode: SafeMode,
    pub source: Source,
}

#[derive(Debug, Default, Clone)]
pub struct GeneratorMetadata {
    pub name: String,
    pub version: String,
}

impl GeneratorMetadata {
    #[must_use]
    pub fn new<S: AsRef<str>>(name: S, version: S) -> Self {
        Self {
            name: name.as_ref().to_string(),
            version: version.as_ref().to_string(),
        }
    }
}

impl std::fmt::Display for GeneratorMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} v{}", self.name, self.version)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum Source {
    Files(Vec<PathBuf>),
    String(String),
    #[default]
    Stdin,
}

pub trait Processable {
    type Config;
    type Error;

    fn new(config: Self::Config) -> Self;

    /// Run the processor
    ///
    /// # Errors
    ///
    /// Will typically return parsing or rendering errors. Implementations are free to
    /// return any error type they wish though.
    fn run(&self) -> Result<(), Self::Error>;

    /// Run the processor but return the processed output as a string
    ///
    /// # Errors
    ///
    /// Will return one of:
    ///
    /// - the processed output
    ///
    /// - parsing or rendering errors. Implementations are free to return any error type
    ///   they wish though.
    fn output(&self) -> Result<String, Self::Error>;
}
