//! Development and testing utilities for acdc converters.
//!
//! This crate provides shared utilities for developing and testing converters:
//!
//! - [`fixtures`] - Source discovery, parsing, and structured fixture observations
//! - [`generate_fixtures`] - Fixture generation for integration tests
//! - [`output`] - Output normalization for test comparisons

pub mod fixtures;
pub mod generate_fixtures;
pub mod output;
