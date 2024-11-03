/// A simple trait for helping in rendering `AsciiDoc` content.
pub trait Render {
    #[allow(clippy::missing_errors_doc)]
    fn render(&self, w: &mut impl std::io::Write) -> std::io::Result<()>;
}

mod block;
mod delimited;
mod document;
mod inline;
mod paragraph;
mod section;
mod table;
