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

#[derive(Debug, Default, Clone, PartialEq)]
pub enum Source {
    Files(Vec<PathBuf>),
    String(String),
    #[default]
    Stdin,
}
