use std::io::Write;

use anyhow::Result;

/// A simple trait for helping in rendering `AsciiDoc` content.
pub trait Render {
    #[allow(clippy::missing_errors_doc)]
    fn render(&self, w: &mut impl std::io::Write) -> std::io::Result<()>;
}

/// Parses a file and renders it to the terminal.
pub fn parse_file(file: &std::path::PathBuf) -> Result<()> {
    let doc = acdc_parser::parse_file(file)?;
    let mut stdout = std::io::stdout();
    doc.render(&mut stdout)?;
    stdout.flush()?;
    Ok(())
}

mod block;
mod delimited;
mod document;
mod inline;
mod paragraph;
mod section;
mod table;
