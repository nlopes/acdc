use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

use acdc_parser::{Options, Parser};

struct CountingAllocator {
    alloc_count: AtomicUsize,
    alloc_bytes: AtomicUsize,
}

impl CountingAllocator {
    const fn new() -> Self {
        Self {
            alloc_count: AtomicUsize::new(0),
            alloc_bytes: AtomicUsize::new(0),
        }
    }

    fn reset(&self) {
        self.alloc_count.store(0, Ordering::Relaxed);
        self.alloc_bytes.store(0, Ordering::Relaxed);
    }

    fn count(&self) -> usize {
        self.alloc_count.load(Ordering::Relaxed)
    }

    fn bytes(&self) -> usize {
        self.alloc_bytes.load(Ordering::Relaxed)
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.alloc_count.fetch_add(1, Ordering::Relaxed);
        self.alloc_bytes.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator::new();

fn main() {
    let fixtures = vec![
        ("basic_header", "acdc-parser/fixtures/tests/basic_header.adoc"),
        ("stem_blocks", "acdc-parser/fixtures/tests/stem_blocks.adoc"),
        (
            "video_comprehensive",
            "acdc-parser/fixtures/tests/video_comprehensive.adoc",
        ),
        ("inline_heavy", "acdc-parser/fixtures/tests/inline_heavy.adoc"),
        ("ARCHITECTURE", "ARCHITECTURE.adoc"),
    ];

    // Warm up — parse once to initialize any lazy statics / caches
    if let Ok(content) = fs::read_to_string("acdc-parser/fixtures/tests/basic_header.adoc") {
        let _ = Parser::new(&content).parse();
    }

    println!(
        "{:<25} {:>8} {:>10} {:>12}",
        "Fixture", "Size", "Allocs", "Bytes"
    );
    println!("{}", "-".repeat(58));

    for (name, path) in &fixtures {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("skip {name}: {e}");
                continue;
            }
        };

        let (median_count, median_bytes) = measure(|| {
            let parser = Parser::new(&content);
            let _ = parser.parse();
        });

        println!(
            "{:<25} {:>7}B {:>10} {:>12}",
            name,
            content.len(),
            format_count(median_count),
            format_bytes(median_bytes),
        );
    }

    // Inline-only breakdown
    let options = Options::default();

    println!("\n--- Inline parsing cost scaling ---\n");
    println!(
        "{:<40} {:>10} {:>12}",
        "Input", "Allocs", "Bytes"
    );
    println!("{}", "-".repeat(65));

    // Measure overhead: what does parse_inline cost for minimal input?
    let (c, b) = measure(|| {
        let _ = acdc_parser::parse_inline("x", &options);
    });
    println!("{:<40} {:>10} {:>12}", "single char 'x'", format_count(c), format_bytes(b));

    // Measure: empty string
    let (c, b) = measure(|| {
        let _ = acdc_parser::parse_inline("", &options);
    });
    println!("{:<40} {:>10} {:>12}", "empty string", format_count(c), format_bytes(b));

    // Measure scaling: 10 words of plain text
    let (c, b) = measure(|| {
        let _ = acdc_parser::parse_inline("one two three four five six seven eight nine ten", &options);
    });
    println!("{:<40} {:>10} {:>12}", "10 plain words", format_count(c), format_bytes(b));

    // Measure: single bold
    let (c, b) = measure(|| {
        let _ = acdc_parser::parse_inline("*bold*", &options);
    });
    println!("{:<40} {:>10} {:>12}", "single *bold*", format_count(c), format_bytes(b));

    // Measure: 10 bolds
    let (c, b) = measure(|| {
        let _ = acdc_parser::parse_inline("*a* *b* *c* *d* *e* *f* *g* *h* *i* *j*", &options);
    });
    println!("{:<40} {:>10} {:>12}", "10x *bold*", format_count(c), format_bytes(b));

    println!("\n--- Per-fragment type (representative lines) ---\n");
    println!(
        "{:<40} {:>10} {:>12}",
        "Fragment", "Allocs", "Bytes"
    );
    println!("{}", "-".repeat(65));

    let inline_fragments = vec![
        ("plain text (no formatting)", "This is a very long section of plain text that contains no special formatting whatsoever and the parser must check every single character."),
        ("dense formatting", "*bold1* text *bold2* text _ital1_ text `mono1` text #mark1# text *bold3* _ital2_ `mono2` #mark2# text."),
        ("cross-references", "See <<section-one>> for details. Also see <<section-two,Section Two>> for more. Reference xref:other-doc.adoc[another document]."),
        ("index terms", "(((primary term))) Here is text. And ((visible index term)) appears inline. Also (((term A))) and (((term B))) scattered."),
        ("escaped syntax", r"Use \*not bold* and \_not italic_ and \`not mono` and \#not highlight#. Escaped \<<not-a-ref>> and \[[not-an-anchor]]."),
        ("mixed all types", "*bold*, _italic_, `monospace`, #highlight#, ^super^, ~sub~, <<section-one>>, ((index)), (((concealed))), and plain text."),
    ];

    for (label, text) in &inline_fragments {
        let (median_count, median_bytes) = measure(|| {
            let _ = acdc_parser::parse_inline(text, &options);
        });

        println!(
            "{:<40} {:>10} {:>12}",
            label,
            format_count(median_count),
            format_bytes(median_bytes),
        );
    }
}

fn measure(f: impl Fn()) -> (usize, usize) {
    let mut counts = Vec::new();
    let mut bytes = Vec::new();
    for _ in 0..5 {
        ALLOCATOR.reset();
        f();
        counts.push(ALLOCATOR.count());
        bytes.push(ALLOCATOR.bytes());
    }
    counts.sort_unstable();
    bytes.sort_unstable();
    (counts[2], bytes[2])
}

fn format_count(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

fn format_bytes(n: usize) -> String {
    if n >= 1_048_576 {
        format!("{:.1} MiB", n as f64 / 1_048_576.0)
    } else if n >= 1_024 {
        format!("{:.1} KiB", n as f64 / 1_024.0)
    } else {
        format!("{n} B")
    }
}
