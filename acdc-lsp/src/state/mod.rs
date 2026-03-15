//! State management for open documents

mod document;
mod workspace;
pub mod xref_target;

pub use document::{
    ConditionalBlock, ConditionalDirectiveKind, ConditionalOperation, DocumentState,
};
pub use workspace::Workspace;
pub use xref_target::XrefTarget;
