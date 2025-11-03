use std::{fs, hint::black_box};

use acdc_parser::Parser;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn parse_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");

    let fixture_files_without_ext = vec!["basic_header", "stem_blocks", "video_comprehensive"];

    for name in fixture_files_without_ext {
        let content = fs::read_to_string(format!("fixtures/tests/{name}.adoc"))
            .expect("Failed to read benchmark fixture file");
        group.bench_with_input(BenchmarkId::new("parse", name), &content, |b, input| {
            b.iter(|| {
                let parser = Parser::new(black_box(input));
                black_box(parser.parse())
            });
        });
    }

    // Additional benchmark with a larger file
    let content =
        fs::read_to_string("../ARCHITECTURE.adoc").expect("Failed to read benchmark fixture file");
    group.bench_with_input(
        BenchmarkId::new("parse", "ARCHITECTURE"),
        &content,
        |b, input| {
            b.iter(|| {
                let parser = Parser::new(black_box(input));
                black_box(parser.parse())
            });
        },
    );

    group.finish();
}

criterion_group!(benches, parse_benchmark);
criterion_main!(benches);
