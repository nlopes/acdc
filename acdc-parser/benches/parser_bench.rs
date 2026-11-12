use std::{fmt::Write as _, fs, hint::black_box, time::Duration};

use acdc_parser::{
    Options, Parser,
    extensions::{BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroInvocation, Registry},
    parse, parse_file,
};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

#[allow(clippy::expect_used)]
fn parse_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");

    group.bench_function("parse_inline/control", |b| {
        b.iter(|| {
            black_box(
                Parser::new(black_box("Plain words with *strong* and _emphasis_.")).parse_inline(),
            )
        });
    });

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

fn inline_scanning_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("inline_scanning");
    for (name, input) in [
        (
            "plain_ascii",
            "Ordinary words and punctuation, with 123 numbers. ".repeat(64),
        ),
        (
            "macro_near_misses",
            "anchors: imagery: keyboards: menus: stems: xreferences: ".repeat(64),
        ),
        (
            "sparse_typography",
            format!(
                "{}Copyright (C), a word--pair, and an arrow ->.",
                "Ordinary words and numbers 123. ".repeat(64)
            ),
        ),
        (
            "dense_typography",
            "(C) (R) (TM) ... word--pair don't -> <= &lt;- ".repeat(64),
        ),
        (
            "unicode",
            "Élan, Ελληνικά, 日本語 and *formatted* text. ".repeat(64),
        ),
    ] {
        assert!(Parser::new(&input).parse_inline().is_ok());
        group.bench_with_input(
            BenchmarkId::new("parse_inline", name),
            &input,
            |b, input| {
                b.iter(|| black_box(Parser::new(black_box(input)).parse_inline()));
            },
        );
    }
    group.finish();
}

fn attribute_declaration_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_attributes");
    let header = format!(
        "= Attributes\n:source: value\n{}\nParagraph.\n",
        ":value: {source}\n".repeat(200)
    );
    let metadata_text = format!(
        "= Attributes\n:source: value\n\n{}",
        "[.role]\n:value: {source}\nParagraph {value}.\n\n".repeat(200)
    );
    let metadata_presence = format!(
        "= Attributes\n\n{}",
        "[.role]\n:flag:\n:!flag:\nParagraph.\n\n".repeat(200)
    );
    for (name, input) in [
        ("header_200", header),
        ("metadata_text_200", metadata_text),
        ("metadata_presence_200", metadata_presence),
    ] {
        assert!(Parser::new(&input).parse().is_ok());
        group.bench_with_input(BenchmarkId::new("parse", name), &input, |b, input| {
            b.iter(|| black_box(Parser::new(black_box(input)).parse()));
        });
    }
    group.finish();
}

#[allow(clippy::expect_used)]
fn diagnostic_collection_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("diagnostic_collection");
    let options = Options::default();
    for (name, source, warnings) in [
        (
            "quiet",
            "A paragraph with *bold*, _emphasis_, and `code`.\n\n".repeat(1000),
            0,
        ),
        (
            "duplicate_ids",
            "[[duplicate]]\nA *formatted* paragraph.\n\n".repeat(1000),
            999,
        ),
    ] {
        let file = tempfile::NamedTempFile::new().expect("create benchmark input");
        fs::write(file.path(), &source).expect("write benchmark input");
        assert_eq!(
            parse(&source, &options)
                .expect("valid benchmark")
                .warnings()
                .len(),
            warnings
        );
        assert_eq!(
            parse_file(file.path(), &options)
                .expect("valid benchmark file")
                .warnings()
                .len(),
            warnings
        );
        group.bench_with_input(BenchmarkId::new("string", name), &source, |b, input| {
            b.iter(|| black_box(parse(black_box(input), &options)));
        });
        group.bench_with_input(BenchmarkId::new("file", name), file.path(), |b, input| {
            b.iter(|| black_box(parse_file(black_box(input), &options)));
        });
    }
    group.finish();
}

struct CheapMacro;

impl BlockMacroProcessor for CheapMacro {
    fn process(&mut self, invocation: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
        black_box(invocation);
        Ok(BlockMacroOutput::None)
    }
}

#[allow(clippy::expect_used)]
fn extension_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("extensions");
    let options = Options::default();
    let ordinary = include_str!("../fixtures/tests/basic_header.adoc");
    let empty = Registry::default();
    let mut builder = Registry::builder();
    for index in 0..100 {
        builder
            .register(BlockMacro::new(format!("cheap{index}"), || CheapMacro))
            .expect("valid benchmark registration");
    }
    let registered = builder.build();
    group.bench_function("empty_registry", |b| {
        b.iter(|| black_box(empty.parse(black_box(ordinary), &options)));
    });
    group.bench_function("unused_registrations_100", |b| {
        b.iter(|| black_box(registered.parse(black_box(ordinary), &options)));
    });
    let macros = "cheap0::target[]\n\n".repeat(1000);
    assert!(registered.parse(&macros, &options).is_ok());
    group.bench_function("cheap_callbacks_1000", |b| {
        b.iter(|| black_box(registered.parse(black_box(&macros), &options)));
    });
    let mut nested = String::new();
    for section in 0..1000 {
        writeln!(
            nested,
            "== Section {section}\n\n====\n****\n* Parent\n** Child\n****\n====\n"
        )
        .expect("write benchmark input");
    }
    assert!(parse(&nested, &options).is_ok());
    group.bench_function("large_nested", |b| {
        b.iter(|| black_box(parse(black_box(&nested), &options)));
    });
    group.finish();
}

// This only uses the standard library. It controls for machine drift and
// binary layout without exercising the parser's changed coordinate adapters.
#[inline(never)]
fn source_line_lengths(input: &str) -> usize {
    input.lines().map(str::len).sum()
}

#[inline(never)]
fn source_checksum(input: &str) -> u64 {
    input.bytes().fold(0_u64, |checksum, byte| {
        checksum.wrapping_mul(31).wrapping_add(u64::from(byte))
    })
}

fn unchanged_control_benchmark(c: &mut Criterion) {
    let input = include_str!("../../ARCHITECTURE.adoc");
    c.bench_function("unchanged/source_line_lengths", |b| {
        b.iter(|| black_box(source_line_lengths(black_box(input))));
    });
    c.bench_function("unchanged/source_checksum", |b| {
        b.iter(|| black_box(source_checksum(black_box(input))));
    });
}

criterion_group!(
    benches,
    parse_benchmark,
    inline_scanning_benchmark,
    attribute_declaration_benchmark,
    diagnostic_collection_benchmark,
    extension_benchmark,
    unchanged_control_benchmark
);
criterion_main!(benches);
