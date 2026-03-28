//! acdc-lsp library
//!
//! Provides the LSP backend implementation for `AsciiDoc` documents.

// The `deprecated` field on DocumentSymbol and SymbolInformation is required by the struct
// but deprecated in the LSP spec in favor of `tags`. Suppress until tower-lsp-server updates.
#![allow(deprecated)]

pub(crate) mod backend;
pub(crate) mod capabilities;
pub(crate) mod convert;
pub(crate) mod state;

pub use backend::Backend;
