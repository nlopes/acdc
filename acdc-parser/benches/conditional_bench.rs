//! Active and inactive block-conditional preprocessing benchmark.
//!
//! Disabled by default (`bench = false` / `test = false` in `Cargo.toml`). Run
//! it explicitly when changing conditional preprocessing:
//!
//! ```text
//! cargo bench --bench conditional_bench
//! ```
//!
//! `active` and `inactive` parse byte-identical inputs with only the document
//! attributes changed. `plain_control` bypasses preprocessing, while
//! `slow_path_control` forces the ordinary preprocessor rebuild without using a
//! conditional. Keep the controls when comparing revisions so uniform machine
//! or codegen shifts are distinguishable from conditional-path changes.
//!
//! For an acceptance comparison, put this same benchmark in the old and new
//! worktrees, then run
//! `python3 acdc-parser/benches/compare_conditionals.py --old OLD --new NEW`.
//! The runner performs seven alternating pairs and fails unless both active and
//! inactive cases improve by at least 2% after adjustment by `plain_control`.

use std::{fmt::Write as _, hint::black_box};

use acdc_parser::{Options, parse};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const LINE_COUNTS: [usize; 2] = [1_000, 10_000];

fn push_listing(document: &mut String, line_count: usize) {
    document.push_str("....\n");
    for line in 0..line_count {
        let _ = writeln!(
            document,
            "line {line:05}: fixed conditional benchmark payload"
        );
    }
    document.push_str("....\n");
}

fn conditional_document(line_count: usize) -> String {
    let mut document = String::with_capacity(line_count * 50);
    document.push_str("ifdef::bench-active[]\n");
    push_listing(&mut document, line_count);
    document.push_str("endif::bench-active[]\n");
    document
}

fn plain_control_document(line_count: usize) -> String {
    let mut document = String::with_capacity(line_count * 50);
    push_listing(&mut document, line_count);
    document
}

fn slow_path_control_document(line_count: usize) -> String {
    let mut document = String::with_capacity(line_count * 50);
    document.push_str(":bench-note: fixed \\\ncontinuation\n\n");
    push_listing(&mut document, line_count);
    document
}

fn conditional_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("conditionals");

    let active_options = Options::builder()
        .with_attribute("bench-active", true)
        .build();
    let inactive_options = Options::default();

    for line_count in LINE_COUNTS {
        let conditional = conditional_document(line_count);
        let plain_control = plain_control_document(line_count);
        let slow_path_control = slow_path_control_document(line_count);

        assert!(parse(&conditional, &active_options).is_ok());
        assert!(parse(&conditional, &inactive_options).is_ok());
        assert!(parse(&plain_control, &inactive_options).is_ok());
        assert!(parse(&slow_path_control, &inactive_options).is_ok());

        group.bench_with_input(
            BenchmarkId::new("active", line_count),
            &conditional,
            |b, input| b.iter(|| black_box(parse(black_box(input), &active_options))),
        );
        group.bench_with_input(
            BenchmarkId::new("inactive", line_count),
            &conditional,
            |b, input| b.iter(|| black_box(parse(black_box(input), &inactive_options))),
        );
        group.bench_with_input(
            BenchmarkId::new("plain_control", line_count),
            &plain_control,
            |b, input| b.iter(|| black_box(parse(black_box(input), &inactive_options))),
        );
        group.bench_with_input(
            BenchmarkId::new("slow_path_control", line_count),
            &slow_path_control,
            |b, input| b.iter(|| black_box(parse(black_box(input), &inactive_options))),
        );
    }

    group.finish();
}

criterion_group!(benches, conditional_benchmark);
criterion_main!(benches);
