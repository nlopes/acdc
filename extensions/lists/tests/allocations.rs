use std::{alloc::System, fmt::Write, hint::black_box};

use acdc_parser::Options;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn repeated_lists_stay_within_the_edit_allocation_budget() -> Result<(), Box<dyn std::error::Error>>
{
    let registry = acdc_extensions_lists::registry()?;
    let options = Options::default();
    let mut images = String::new();
    for number in 0..100 {
        writeln!(images, ".Image {number}\nimage::target.svg[]\n")?;
    }
    let measure = |requests| -> Result<usize, acdc_parser::Error> {
        let source = "list-of::image[]\n\n".repeat(requests) + &images;
        let region = Region::new(GLOBAL);
        let _ = black_box(registry.parse(&source, &options)?.finish()?);
        Ok(region.change().bytes_allocated)
    };
    measure(1)?;
    let ten = measure(10)?;
    let hundred = measure(100)?;
    // Ten times the output, with headroom for allocator and target differences.
    // Copying all earlier generated paragraphs on every edit exceeds this budget.
    assert!(
        hundred <= 16 * ten,
        "10 requests: {ten} bytes, 100: {hundred} bytes"
    );
    Ok(())
}
