//! acdc-lsp library
//!
//! Provides the LSP backend implementation for `AsciiDoc` documents.

// The `deprecated` field on DocumentSymbol and SymbolInformation is required by the struct
// but deprecated in the LSP spec in favor of `tags`. Suppress until tower-lsp updates.
#![allow(deprecated)]

pub mod backend;
pub mod capabilities;
pub mod convert;
pub mod state;

pub use backend::Backend;
