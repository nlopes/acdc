//! State management for open documents

mod document;
mod workspace;
pub mod xref_target;

pub(crate) use document::extract_includes;
pub use document::{
    ConditionalBlock, ConditionalDirectiveKind, ConditionalOperation, DocumentState,
};
pub use workspace::Workspace;
pub use xref_target::XrefTarget;
