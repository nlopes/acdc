//! Allocation-sensitive parser tests.
//!
//! `stats_alloc` counts allocations process-wide, so a measured region also sees
//! anything another thread allocates meanwhile: other tests and the libtest
//! harness itself. This binary therefore uses a custom harness that runs every
//! test on the main thread, one at a time, under both `cargo test` and nextest.
//!
//! Register each test in its module's `trials()`. Test functions carry no
//! `#[test]` attribute, so `deny(dead_code)` rejects one that is not registered.

#![deny(dead_code)]

use std::{alloc::System, fmt::Display, process::ExitCode};

use acdc_parser::CompatibilityMode;
use libtest_mimic::{Arguments, Failed, Trial};
use stats_alloc::{INSTRUMENTED_SYSTEM, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

/// Register a module's tests. `tests` take no arguments; `modes` take a
/// [`CompatibilityMode`] and run once per mode.
macro_rules! trials {
    (
        $(tests: [$($test:ident),* $(,)?],)?
        $(modes: [$($moded:ident),* $(,)?],)?
    ) => {{
        let module = crate::module_name(module_path!());
        let mut trials = Vec::new();
        $($(trials.push(crate::trial(format!("{module}::{}", stringify!($test)), $test));)*)?
        $($(trials.extend(crate::mode_trials(&format!("{module}::{}", stringify!($moded)), $moded));)*)?
        trials
    }};
}

mod attribute_value_text;
mod conditional_allocations;
mod no_leak;
mod owned_atom_edits;
mod owned_catalog_edits;
mod owned_content_edits;
mod owned_header_copies;
mod owned_hyperlink_edits;
mod owned_inline_media_edits;
mod owned_inline_selection;
mod owned_list_item_edits;
mod owned_macro_edits;
mod owned_note_edits;
mod owned_passthrough_edits;
mod owned_span_edits;
mod owned_table_edits;
mod read_ast_attribute_allocations;
mod read_ast_captions;
mod read_ast_catalogs;
mod read_ast_content_allocations;
mod read_ast_identity;
mod read_ast_location_equality;
mod read_ast_traversal_allocations;
mod stem_allocations;

fn main() -> ExitCode {
    let mut args = Arguments::from_args();
    // Nothing else may allocate while a test measures, including the harness.
    args.test_threads = Some(1);
    let trials = [
        attribute_value_text::trials(),
        conditional_allocations::trials(),
        no_leak::trials(),
        owned_atom_edits::trials(),
        owned_catalog_edits::trials(),
        owned_content_edits::trials(),
        owned_header_copies::trials(),
        owned_hyperlink_edits::trials(),
        owned_inline_media_edits::trials(),
        owned_inline_selection::trials(),
        owned_list_item_edits::trials(),
        owned_macro_edits::trials(),
        owned_note_edits::trials(),
        owned_passthrough_edits::trials(),
        owned_span_edits::trials(),
        owned_table_edits::trials(),
        read_ast_attribute_allocations::trials(),
        read_ast_captions::trials(),
        read_ast_catalogs::trials(),
        read_ast_content_allocations::trials(),
        read_ast_identity::trials(),
        read_ast_location_equality::trials(),
        read_ast_traversal_allocations::trials(),
        stem_allocations::trials(),
    ]
    .into_iter()
    .flatten()
    .collect();
    libtest_mimic::run(&args, trials).exit_code()
}

/// The module path below the crate root, used as the test name prefix.
fn module_name(path: &'static str) -> &'static str {
    path.split_once("::").map_or(path, |(_, module)| module)
}

/// A test result that libtest-mimic can report.
trait Outcome {
    fn into_result(self) -> Result<(), Failed>;
}

impl Outcome for () {
    fn into_result(self) -> Result<(), Failed> {
        Ok(())
    }
}

impl<E: Display> Outcome for Result<(), E> {
    fn into_result(self) -> Result<(), Failed> {
        self.map_err(Failed::from)
    }
}

fn trial<O: Outcome>(name: String, test: impl FnOnce() -> O + Send + 'static) -> Trial {
    Trial::test(name, move || test().into_result())
}

fn mode_trials<O: Outcome>(
    name: &str,
    test: impl Fn(CompatibilityMode) -> O + Copy + Send + 'static,
) -> [Trial; 2] {
    [
        trial(format!("{name}::asciidoctor"), move || {
            test(CompatibilityMode::Asciidoctor)
        }),
        trial(format!("{name}::draft"), move || {
            test(CompatibilityMode::Draft)
        }),
    ]
}
