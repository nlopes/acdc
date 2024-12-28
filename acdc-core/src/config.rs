use std::path::PathBuf;

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

#[derive(Debug)]
pub struct Config {
    pub doctype: Doctype,
    pub safe_mode: SafeMode,
    pub files: Vec<PathBuf>,
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
}
