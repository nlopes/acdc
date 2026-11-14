//! Allocation budgets for ordinary parsing of unchanged selected includes.
//! Keep one test: the allocator region measures the entire process.

use std::{alloc::System, fmt::Write as _, hint::black_box, process::Command};

use acdc_parser::{CompatibilityMode, IncludeLoader, Options, SafeMode, parse_file};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn unchanged_selected_include_allocation_budgets() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("SOURCE_DATE_EPOCH").as_deref() != Ok("0") {
        let status = Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg("unchanged_selected_include_allocation_budgets")
            .arg("--nocapture")
            .env("SOURCE_DATE_EPOCH", "0")
            .status()?;
        assert!(status.success());
        return Ok(());
    }

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("main.adoc");
    std::fs::write(
        directory.path().join("part.adoc"),
        "ignored\n// tag::body[]\nFirst selected line.\nSecond selected line.\n// end::body[]\nignored\n",
    )?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Safe)
            .with_include_loader(IncludeLoader::System)
            .build()?;
        for selection in ["tag=body", "lines=3..4"] {
            // Main's include-attribute decoding adds two allocations per include.
            // These bounds retain the fixed overhead and match the updated baseline.
            for (includes, budget) in [(1, 132), (10, 627), (50, 2_827), (200, 11_077)] {
                let mut source = String::from("= Includes\n\n");
                for _ in 0..includes {
                    writeln!(source, "include::part.adoc[{selection}]\n")?;
                }
                std::fs::write(&path, source)?;
                let _ = black_box(parse_file(&path, &options)?);
                let region = Region::new(GLOBAL);
                let parsed = parse_file(&path, &options)?;
                let stats = region.change();
                assert!(
                    stats.allocations <= budget,
                    "{mode:?}, {selection}, {includes} includes: {stats:?}"
                );
                assert_eq!(parsed.document().blocks().count(), includes);
                let _ = black_box(parsed);
            }
        }
    }
    Ok(())
}
