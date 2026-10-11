use std::{alloc::System, hint::black_box};

use acdc_parser::{Options, parse};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn stem_preprocessing_allocation_budgets() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::default();
    let prose = "Plain words with *strong* and _emphasis_. ".repeat(100);
    // Byte budgets allow small target-dependent differences in type layout.
    for (name, source, allocations, reallocations, bytes) in [
        ("prose", prose.as_str(), 822, 8, 355_576),
        ("formula", "Before stem:[x^2] after.", 23, 0, 7_464),
        (
            "protected",
            ":value: expanded\n\nBefore stem:[{value} *x* +y+ --] after {value}.",
            49,
            4,
            9_995,
        ),
    ] {
        let _ = black_box(parse(source, &options)?);
        let region = Region::new(GLOBAL);
        let parsed = parse(source, &options)?;
        let stats = region.change();
        assert!(stats.allocations <= allocations, "{name}: {stats:?}");
        assert!(stats.reallocations <= reallocations, "{name}: {stats:?}");
        assert!(stats.bytes_allocated <= bytes + 128, "{name}: {stats:?}");
        let _ = black_box(parsed);
    }
    Ok(())
}
