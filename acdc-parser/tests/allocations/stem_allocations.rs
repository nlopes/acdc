use std::{hint::black_box, process::Command};

use acdc_parser::{Options, parse};
use stats_alloc::Region;

use super::ALLOCATOR;

fn stem_preprocessing_allocation_budgets() -> Result<(), Box<dyn std::error::Error>> {
    // Pin time in a child process so timestamp-cache refreshes cannot enter
    // the measured region, without changing this process's environment.
    if std::env::var("SOURCE_DATE_EPOCH").as_deref() != Ok("0") {
        let name = format!(
            "{}::stem_preprocessing_allocation_budgets",
            crate::module_name(module_path!())
        );
        let output = Command::new(std::env::current_exe()?)
            .args(["--exact", &name, "--nocapture"])
            .env("SOURCE_DATE_EPOCH", "0")
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{stdout}");
        // A filter that matches nothing also exits successfully.
        assert!(stdout.contains("1 passed"), "child ran no test:\n{stdout}");
        return Ok(());
    }

    let options = Options::default();
    let prose = "Plain words with *strong* and _emphasis_. ".repeat(100);
    // Byte budgets allow small target-dependent differences in type layout.
    for (name, source, allocations, reallocations, bytes) in [
        ("prose", prose.as_str(), 822, 8, 355_880),
        ("formula", "Before stem:[x^2] after.", 23, 0, 7_768),
        (
            "protected",
            ":value: expanded\n\nBefore stem:[{value} *x* +y+ --] after {value}.",
            49,
            4,
            10_406,
        ),
    ] {
        let _ = black_box(parse(source, &options)?);
        let region = Region::new(ALLOCATOR);
        let parsed = parse(source, &options)?;
        let stats = region.change();
        assert!(stats.allocations <= allocations, "{name}: {stats:?}");
        assert!(stats.reallocations <= reallocations, "{name}: {stats:?}");
        assert!(stats.bytes_allocated <= bytes + 128, "{name}: {stats:?}");
        let _ = black_box(parsed);
    }
    Ok(())
}

pub(super) fn trials() -> Vec<libtest_mimic::Trial> {
    trials! {
        tests: [stem_preprocessing_allocation_budgets],
    }
}
