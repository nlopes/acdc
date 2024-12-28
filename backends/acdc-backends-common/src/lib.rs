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
    pub doctype: Doctype,
    pub safe_mode: SafeMode,
    pub source: Source,
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
