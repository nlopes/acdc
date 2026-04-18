//! Regression test: `parse_file` / `parse_inline` must not leak memory across
//! repeated calls.
//!
//! We install `stats_alloc` as the global allocator for this test binary and
//! measure net-resident bytes (allocations minus deallocations) around a loop
//! of parse calls. On the current `Box::leak`-based code path, every call
//! leaks the input buffer plus a bumpalo arena, so the net grows linearly
//! with the iteration count — far beyond the slack budget. Once the
//! `Document::into_static()` refactor lands, the delta drops to within
//! incidental allocator noise.
//!
//! Each integration test file is its own binary in Cargo, so installing a
//! `#[global_allocator]` here does not affect other tests.

use std::{alloc::System, error::Error, path::Path};

use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type TestResult = Result<(), Box<dyn Error>>;

const WARMUP_ITERATIONS: usize = 10;
const MEASURED_ITERATIONS: usize = 200;
/// Slack budget for `parse_file`. The post-`Box::leak` floor is ~4 KB/parse
/// coming from a pre-existing, orthogonal leak in the inline preprocessor's
/// attribute-reference substitution path (tracked separately). A threshold
/// of 2 MB over 200 iterations (~10 KB/parse) sits comfortably above that
/// floor while still catching any regression to `Box::leak`-class growth
/// (historical: ~28 KB/parse from the leaked file buffer + arena).
const PARSE_FILE_SLACK_BYTES: i64 = 2 * 1024 * 1024;
/// Tighter slack for `parse_inline`. No pre-existing leak on this path, so
/// we hold it to a small budget that catches any arena or input-buffer
/// regression.
const PARSE_INLINE_SLACK_BYTES: i64 = 24 * 1024;

fn net_bytes_delta(region: &Region<'_, System>) -> i64 {
    let change = region.change();
    let allocated = i64::try_from(change.bytes_allocated).unwrap_or(i64::MAX);
    let deallocated = i64::try_from(change.bytes_deallocated).unwrap_or(i64::MAX);
    allocated - deallocated
}

/// Parse the mdbasics fixture in a loop and assert that net allocator bytes
/// return close to baseline. Fails loudly on any `Box::leak`-style escape.
#[test]
fn parse_file_does_not_leak_across_iterations() -> TestResult {
    let opts = acdc_parser::Options::builder().build();
    let fixture = Path::new("fixtures/samples/mdbasics/mdbasics.adoc");
    assert!(
        fixture.exists(),
        "test fixture missing: {}",
        fixture.display()
    );

    for _ in 0..WARMUP_ITERATIONS {
        let _doc = acdc_parser::parse_file(fixture, &opts)?;
    }

    let region = Region::new(GLOBAL);
    for _ in 0..MEASURED_ITERATIONS {
        let _doc = acdc_parser::parse_file(fixture, &opts)?;
    }
    let delta = net_bytes_delta(&region);

    let file_size = i64::try_from(std::fs::metadata(fixture)?.len()).unwrap_or(i64::MAX);
    let per_iter = delta / i64::try_from(MEASURED_ITERATIONS).unwrap_or(1);

    assert!(
        delta < PARSE_FILE_SLACK_BYTES,
        "parse_file appears to leak memory: net grew {delta} bytes over \
         {MEASURED_ITERATIONS} iterations (~{per_iter} bytes/parse). \
         Fixture is {file_size} bytes. Slack budget is {PARSE_FILE_SLACK_BYTES} bytes. \
         Expected net allocator bytes to stay flat after parse results drop.",
    );
    Ok(())
}

/// Parse inline content repeatedly and assert the same invariant for the
/// `parse_inline` entry point, which maintains its own leaked arena today.
#[test]
fn parse_inline_does_not_leak_across_iterations() -> TestResult {
    let opts = acdc_parser::Options::builder()
        .with_attribute("name", "World")
        .build();
    // Mix of substitution, passthrough, and nested macros — forces the
    // inline preprocessor onto its non-fast-path, which is where the
    // bumpalo arena grows.
    let input = "Hello {name}, here is *strong* _emphasized_ `mono` text \
                 with pass:[raw 1<2] and https://example.com[a link] and \
                 a footnote:[a note with {name} interpolation].";

    for _ in 0..WARMUP_ITERATIONS {
        let _nodes = acdc_parser::parse_inline(input, &opts)?;
    }

    let region = Region::new(GLOBAL);
    for _ in 0..MEASURED_ITERATIONS {
        let _nodes = acdc_parser::parse_inline(input, &opts)?;
    }
    let delta = net_bytes_delta(&region);
    let per_iter = delta / i64::try_from(MEASURED_ITERATIONS).unwrap_or(1);

    assert!(
        delta < PARSE_INLINE_SLACK_BYTES,
        "parse_inline appears to leak memory: net grew {delta} bytes over \
         {MEASURED_ITERATIONS} iterations (~{per_iter} bytes/parse). \
         Slack budget is {PARSE_INLINE_SLACK_BYTES} bytes.",
    );
    Ok(())
}

/// Compile-time guarantee that `parse_file` returns a truly `'static` value
/// that can outlive any local buffer. Fails to compile if the signature ever
/// regresses to a borrowed lifetime.
#[test]
fn parse_file_returns_static_document() -> TestResult {
    fn assert_static<T: 'static>(_: &T) {}

    let opts = acdc_parser::Options::builder().build();
    let doc = acdc_parser::parse_file("fixtures/samples/mdbasics/mdbasics.adoc", &opts)?;
    assert_static(&doc);
    Ok(())
}
