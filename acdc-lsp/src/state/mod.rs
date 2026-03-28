//! State management for open documents

mod document;
mod workspace;
pub(crate) mod xref_target;

pub(crate) use document::extract_includes;
pub(crate) use document::{
    ConditionalBlock, ConditionalDirectiveKind, ConditionalOperation, DocumentState,
};
pub(crate) use workspace::Workspace;
pub(crate) use xref_target::XrefTarget;
