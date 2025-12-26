//! acdc-lsp library
//!
//! Provides the LSP backend implementation for `AsciiDoc` documents.

pub mod backend;
pub mod capabilities;
pub mod convert;
pub mod state;

pub use backend::Backend;
