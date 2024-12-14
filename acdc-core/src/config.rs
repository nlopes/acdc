use std::path::Path;

use clap::ValueEnum;

/// document type to use when converting document
#[derive(Debug, Clone, ValueEnum)]
pub enum Doctype {
    Article,
    Book,
    Manpage,
    Inline,
}

/// safe mode to use when converting document
#[derive(Debug, Clone, ValueEnum)]
pub enum SafeMode {
    Safe,
    Unsafe,
    Server,
    Secure,
}

pub struct Config {
    pub doctype: Doctype,
    pub safe_mode: SafeMode,
}

pub trait Processable {
    type Config;
    type Error;

    fn new(config: Self::Config) -> Self;

    /// Process a list of files
    ///
    /// # Errors
    ///
    /// Will typically return parsing or rendering errors. Implementations are free to
    /// return any error type they wish though.
    fn process_files<P: AsRef<Path>>(&self, files: &[P]) -> Result<(), Self::Error>;
}
