//! Property-based tests for the ACDC parser
//!
//! These tests verify invariants that should hold for ANY input, not just
//! specific fixtures. They complement the fixture-based tests by finding
//! edge cases and ensuring the parser is robust against unexpected inputs.

mod generators;
mod invariants;
