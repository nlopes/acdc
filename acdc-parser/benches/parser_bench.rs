use std::{fs, hint::black_box, time::Duration};

use acdc_parser::Parser;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

#[allow(clippy::expect_used)]
fn parse_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");

    let fixture_files_without_ext = vec![
        "basic_header",
        "stem_blocks",
        "video_comprehensive",
        "inline_heavy",
        "document_attributes_nested",
        "passthrough_nested_markup",
        "table_cell_styles",
        "table_cell_colspan",
        "table_cell_duplication",
        "table_cell_span_combined",
        "table_csv_multiline",
        "table_nested_basic",
    ];

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

    let duplicated_table = format!(
        ":name: World\n\n[cols=\"3*\"]\n|===\n{}|===\n",
        "3*|Hello *{name}* and _text_.\n".repeat(500)
    );
    assert!(Parser::new(&duplicated_table).parse().is_ok());
    group.bench_with_input(
        BenchmarkId::new("parse", "table_duplication_500"),
        &duplicated_table,
        |b, input| b.iter(|| black_box(Parser::new(black_box(input)).parse())),
    );

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

    // Large-document size ladder: isolates pure parser time (no CLI startup,
    // no file IO, no HTML rendering) so we can measure the asymptotic gap
    // against asciidoctor without the noise of a full `convert` pipeline.
    for size in &["5KB", "50KB", "macros_50KB", "250KB", "500KB", "1MB"] {
        let path = format!("fixtures/samples/different-sizes/test_sample_{size}.adoc");
        if let Ok(content) = fs::read_to_string(&path) {
            if *size == "1MB" {
                group.measurement_time(Duration::from_secs(10));
            }
            group.bench_with_input(
                BenchmarkId::new("parse", format!("sample_{size}")),
                &content,
                |b, input| {
                    b.iter(|| {
                        let parser = Parser::new(black_box(input));
                        black_box(parser.parse())
                    });
                },
            );
        } else {
            eprintln!("skipping missing fixture: {path}");
        }
    }

    group.finish();
}

criterion_group!(benches, parse_benchmark);
criterion_main!(benches);
