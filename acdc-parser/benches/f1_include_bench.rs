//! Include / partial-include parse benchmark.
//!
//! **Disabled by default** (`bench = false` / `test = false` in `Cargo.toml`) so
//! `cargo bench`, `cargo test`, and CI never run it — it writes temp fixtures and
//! takes minutes. Run it explicitly when assessing include-path changes:
//!
//! ```text
//! cargo bench --bench f1_include_bench
//! # before/after comparison (drift-prone — prefer a paired/alternating run):
//! cargo bench --bench f1_include_bench -- --save-baseline before   # on the old code
//! cargo bench --bench f1_include_bench -- --baseline   before      # on the new code
//! ```
//!
//! Three groups:
//! - `corpus`  — aggregate parse of every fixture `.adoc` (the common, no-include
//!   path my change does not touch — guards against regression there).
//! - `sizes`   — the big-file size ladder (5KB … 1MB) + ARCHITECTURE.adoc.
//! - `includes`— synthetic include-heavy docs parsed via `parse_file` (the path the
//!   F1 change adds work to: per-line origin tracking + per-run `SourceRange`s).
//!
//! The synthetic include corpus is generated deterministically into a temp dir, so
//! the inputs are byte-identical between the "before" and "after" runs.

use std::{fs, hint::black_box, path::Path, path::PathBuf, time::Duration};

use acdc_parser::{Options, parse, parse_file};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

/// Recursively collect every `*.adoc` under `dir`, excluding the fixtures added by
/// the F1 change (absent from the "before" tree) so both runs parse an identical set.
fn collect_adoc(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_adoc(&path, out);
        } else if path.extension().is_some_and(|e| e == "adoc") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Excluded so the corpus set matches the pre-F1 tree exactly.
            if !name.starts_with("include_partial_") {
                out.push(path);
            }
        }
    }
}

/// Deterministic synthetic include corpus, written once into a temp dir.
struct IncludeCorpus {
    dir: PathBuf,
    whole_file: PathBuf,
    many_includes: PathBuf,
    partial_lines: PathBuf,
    partial_tags: PathBuf,
}

impl IncludeCorpus {
    #[allow(clippy::expect_used)]
    fn generate() -> Self {
        let dir = std::env::temp_dir().join("acdc_f1_bench");
        fs::create_dir_all(&dir).expect("create bench temp dir");

        // A large included part: 600 two-line paragraphs (no headings, so repeated
        // includes don't collide on section ids). ~1800 lines.
        let mut part = String::new();
        for i in 0..600 {
            part.push_str(&format!(
                "Paragraph {i} opening line with _emphasis_, *strong*, `mono`, and a https://example.com[link].\n"
            ));
            part.push_str(&format!(
                "Paragraph {i} second line continuing the thought.\n\n"
            ));
        }
        fs::write(dir.join("part_big.adoc"), &part).expect("write part_big");

        // Whole-file include (single run; exercises the per-line origin Vec on the
        // common, unfiltered path).
        let whole_file = dir.join("main_whole_file.adoc");
        fs::write(&whole_file, "= Whole File\n\ninclude::part_big.adoc[]\n")
            .expect("write main_whole_file");

        // Many whole-file includes (a book of 80 short chapters).
        for k in 0..80 {
            let mut ch = format!("== Chapter {k}\n\n");
            for p in 0..10 {
                ch.push_str(&format!("Chapter {k} paragraph {p} with some text.\n\n"));
            }
            fs::write(dir.join(format!("chapter_{k}.adoc")), ch).expect("write chapter");
        }
        let mut many = String::from("= Book\n\n");
        for k in 0..80 {
            many.push_str(&format!("include::chapter_{k}.adoc[]\n\n"));
        }
        let many_includes = dir.join("main_many_includes.adoc");
        fs::write(&many_includes, many).expect("write main_many_includes");

        // Many partial line-range includes, each a NON-contiguous selection
        // (`lines=a..b;c..d`) → two runs per include → the path F1 splits.
        let mut partial = String::from("= Partial Lines\n\n");
        for k in 0..100 {
            let a = k * 10 + 1;
            let b = a + 2;
            let c = b + 3;
            let d = c + 2;
            partial.push_str(&format!(
                "include::part_big.adoc[lines={a}..{b};{c}..{d}]\n\n"
            ));
        }
        let partial_lines = dir.join("main_partial_lines.adoc");
        fs::write(&partial_lines, partial).expect("write main_partial_lines");

        // A tagged file with 50 regions, included one tag at a time.
        let mut tagged = String::new();
        for k in 0..50 {
            tagged.push_str(&format!("// tag::t{k}[]\n"));
            tagged.push_str(&format!("Tagged region {k} first line.\n"));
            tagged.push_str(&format!("Tagged region {k} second line.\n"));
            tagged.push_str(&format!("// end::t{k}[]\n"));
        }
        fs::write(dir.join("tagged.adoc"), tagged).expect("write tagged");
        let mut tags_main = String::from("= Partial Tags\n\n");
        for k in 0..50 {
            tags_main.push_str(&format!("include::tagged.adoc[tag=t{k}]\n\n"));
        }
        let partial_tags = dir.join("main_partial_tags.adoc");
        fs::write(&partial_tags, tags_main).expect("write main_partial_tags");

        Self {
            dir,
            whole_file,
            many_includes,
            partial_lines,
            partial_tags,
        }
    }
}

#[allow(clippy::expect_used)]
fn corpus_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("corpus");

    // Aggregate: parse every fixture .adoc in one iteration.
    let mut files = Vec::new();
    collect_adoc(Path::new("fixtures"), &mut files);
    files.sort();
    let contents: Vec<String> = files
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok())
        .collect();
    let n = contents.len();
    group.measurement_time(Duration::from_secs(10));
    group.bench_function(BenchmarkId::new("all_fixtures", n), |b| {
        b.iter(|| {
            let opts = Options::default();
            for content in &contents {
                let _ = black_box(parse(black_box(content), &opts));
            }
        });
    });

    group.finish();
}

#[allow(clippy::expect_used)]
fn sizes_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("sizes");

    if let Ok(content) = fs::read_to_string("../ARCHITECTURE.adoc") {
        group.bench_with_input(
            BenchmarkId::new("parse", "ARCHITECTURE"),
            &content,
            |b, input| {
                b.iter(|| black_box(parse(black_box(input), &Options::default())));
            },
        );
    }

    for size in &["5KB", "50KB", "macros_50KB", "250KB", "500KB", "1MB"] {
        let path = format!("fixtures/samples/different-sizes/test_sample_{size}.adoc");
        if let Ok(content) = fs::read_to_string(&path) {
            if *size == "1MB" || *size == "500KB" {
                group.measurement_time(Duration::from_secs(12));
            }
            group.bench_with_input(
                BenchmarkId::new("parse", format!("sample_{size}")),
                &content,
                |b, input| {
                    b.iter(|| black_box(parse(black_box(input), &Options::default())));
                },
            );
        }
    }

    group.finish();
}

fn includes_benchmark(c: &mut Criterion) {
    let corpus = IncludeCorpus::generate();
    let mut group = c.benchmark_group("includes");
    group.measurement_time(Duration::from_secs(8));

    for (name, path) in [
        ("whole_file", &corpus.whole_file),
        ("many_includes", &corpus.many_includes),
        ("partial_lines", &corpus.partial_lines),
        ("partial_tags", &corpus.partial_tags),
    ] {
        group.bench_function(BenchmarkId::new("parse_file", name), |b| {
            b.iter(|| black_box(parse_file(black_box(path), &Options::default())));
        });
    }

    group.finish();
    let _ = &corpus.dir;
}

criterion_group!(
    benches,
    corpus_benchmark,
    sizes_benchmark,
    includes_benchmark
);
criterion_main!(benches);
