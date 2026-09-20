//! Parse results must release temporary buffers as well as the retained AST.
//! Allocation regions measure requested bytes still live after each result drops.
//!
//! Each integration test file is its own binary in Cargo, so installing a
//! `#[global_allocator]` here does not affect other tests.

use acdc_parser::{Options, parse, parse_file};
use std::{alloc::System, error::Error, path::Path};

use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type TestResult = Result<(), Box<dyn Error>>;

const WARMUP_ITERATIONS: usize = 10;
const MEASURED_ITERATIONS: usize = 200;
// Allow fixed cache noise, but not one leaked collection per parse.
const SLACK_BYTES: i64 = 1024;

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
    let opts = Options::builder().build()?;
    let fixture = Path::new("fixtures/samples/mdbasics/mdbasics.adoc");
    assert!(
        fixture.exists(),
        "test fixture missing: {}",
        fixture.display()
    );

    for _ in 0..WARMUP_ITERATIONS {
        let _doc = parse_file(fixture, &opts)?;
    }

    let region = Region::new(GLOBAL);
    for _ in 0..MEASURED_ITERATIONS {
        let _doc = parse_file(fixture, &opts)?;
    }
    let delta = net_bytes_delta(&region);
    eprintln!("parse_file: {:?}; retained={delta}", region.change());

    let file_size = i64::try_from(std::fs::metadata(fixture)?.len()).unwrap_or(i64::MAX);
    let per_iter = delta / i64::try_from(MEASURED_ITERATIONS).unwrap_or(1);

    assert!(
        delta < SLACK_BYTES,
        "parse_file appears to leak memory: net grew {delta} bytes over \
         {MEASURED_ITERATIONS} iterations (~{per_iter} bytes/parse). \
         Fixture is {file_size} bytes. Slack budget is {SLACK_BYTES} bytes. \
         Expected net allocator bytes to stay flat after parse results drop.",
    );
    Ok(())
}

/// Parse inline content repeatedly and assert the same invariant for the
/// `parse_inline` entry point.
#[test]
fn parse_inline_does_not_leak_across_iterations() -> TestResult {
    let opts = Options::builder().with_attribute("name", "World").build()?;
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
    eprintln!("parse_inline: {:?}; retained={delta}", region.change());
    let per_iter = delta / i64::try_from(MEASURED_ITERATIONS).unwrap_or(1);

    assert!(
        delta < SLACK_BYTES,
        "parse_inline appears to leak memory: net grew {delta} bytes over \
         {MEASURED_ITERATIONS} iterations (~{per_iter} bytes/parse). \
         Slack budget is {SLACK_BYTES} bytes.",
    );
    Ok(())
}

#[test]
fn document_inline_temporaries_are_released() -> TestResult {
    let options = Options::builder()
        .with_attribute("name", "World")
        .with_attribute("empty", "")
        .build()?;
    let cases = [
        ("plain", "A plain paragraph.\n", true),
        (
            "declarations",
            "= T\n:source: value\n:copy: {source}\n:present:\n:empty-copy: {empty}\n\
             :max-include-depth: 064\n\n:body: {copy}\n\n{body}\n\n:body!:\n",
            true,
        ),
        (
            "preprocessed_declarations",
            ":enabled:\nifdef::enabled[]\n:source: value\n:copy: {source}\nendif::[]\n\n{copy}\n",
            true,
        ),
        ("attributes", "Hello *{name}* and {empty}.\n", true),
        ("empty", "{empty}\n", true),
        (
            "passthroughs",
            "pass:q[*strong*] and ++raw++ and {lt}.\n",
            true,
        ),
        (
            "nested",
            "link:https://example.com[{name}] and footnote:[*{name}*].\n\n\
             [cols=a]\n|===\n|Nested {name} and pass:q[*text*].\n|===\n",
            true,
        ),
        (
            "error_after_inlines",
            "{name} and pass:[raw].\n\n[cols=1000000*]\n|===\n|cell\n|===\n",
            false,
        ),
    ];
    let mut leaked = false;
    for (name, input, succeeds) in cases {
        for _ in 0..WARMUP_ITERATIONS {
            assert_eq!(parse(input, &options).is_ok(), succeeds, "{name}");
        }
        let region = Region::new(GLOBAL);
        for _ in 0..MEASURED_ITERATIONS {
            assert_eq!(parse(input, &options).is_ok(), succeeds, "{name}");
        }
        let delta = net_bytes_delta(&region);
        let stats = region.change();
        leaked |= delta >= SLACK_BYTES;
        eprintln!("{name}: {stats:?}; retained={delta}");
    }
    assert!(!leaked, "document parsing retained temporary allocations");
    Ok(())
}

/// Compile-time guarantee that `parse_file` returns a truly `'static` value
/// that can outlive any local buffer. Fails to compile if the signature ever
/// regresses to a borrowed lifetime.
#[test]
fn parse_file_returns_static_document() -> TestResult {
    fn assert_static<T: 'static>(_: &T) {}

    let opts = Options::builder().build()?;
    let doc = parse_file("fixtures/samples/mdbasics/mdbasics.adoc", &opts)?;
    assert_static(&doc);
    Ok(())
}

/// Include buffers and their source maps must drop with the parse result.
#[test]
fn included_content_does_not_leak_across_iterations() -> TestResult {
    let options = Options::default();
    let path = Path::new("fixtures/tests/leveloffset_include.adoc");
    for _ in 0..WARMUP_ITERATIONS {
        let parsed = parse_file(path, &options)?;
        assert!(parsed.warnings().is_empty());
    }
    let region = Region::new(GLOBAL);
    for _ in 0..MEASURED_ITERATIONS {
        let _parsed = parse_file(path, &options)?;
    }
    let delta = net_bytes_delta(&region);
    eprintln!("include: {:?}; retained={delta}", region.change());
    assert!(
        delta < SLACK_BYTES,
        "included content retained {delta} bytes"
    );
    Ok(())
}
