use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

use acdc_parser::Parser;

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

        // Run 5 iterations and take the median to reduce noise
        let mut counts = Vec::new();
        let mut bytes = Vec::new();
        for _ in 0..5 {
            ALLOCATOR.reset();
            let parser = Parser::new(&content);
            let _ = parser.parse();
            counts.push(ALLOCATOR.count());
            bytes.push(ALLOCATOR.bytes());
        }
        counts.sort_unstable();
        bytes.sort_unstable();

        let median_count = counts[2];
        let median_bytes = bytes[2];

        println!(
            "{:<25} {:>7}B {:>10} {:>12}",
            name,
            content.len(),
            format_count(median_count),
            format_bytes(median_bytes),
        );
    }
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
