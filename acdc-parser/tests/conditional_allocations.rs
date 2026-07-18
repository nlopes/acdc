//! Deterministic allocation budgets for conditional preprocessing.
//!
//! BEWARE: this was pretty much all written by Claude not me (@nlopes)!
//!
//! Timing benchmarks remain machine-sensitive, so this test protects the
//! underlying work directly: allocation and reallocation counts plus requested
//! bytes. Keep this file to one test because allocator regions are process-wide.

use std::{alloc::System, fmt::Write as _, hint::black_box};

use acdc_parser::{Options, parse};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, Stats, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

// Requested allocation sizes can differ slightly across compiler targets due
// to standard-library type layout. Keep this fixed tolerance small enough that
// per-line or capacity-growth regressions still fail the budget.
const ALLOCATED_BYTE_LAYOUT_TOLERANCE: usize = 128;

#[derive(Clone, Copy)]
struct Budget {
    allocations: usize,
    reallocations: usize,
    bytes_allocated: usize,
    bytes_reallocated: isize,
}

#[derive(Clone, Copy)]
struct ScenarioBudget {
    line_count: usize,
    active: Budget,
    inactive: Budget,
    plain_control: Budget,
    slow_control: Budget,
}

const BUDGETS: [ScenarioBudget; 2] = [
    ScenarioBudget {
        line_count: 1_000,
        active: Budget {
            allocations: 371,
            reallocations: 25,
            bytes_allocated: 261_651,
            bytes_reallocated: 17_384,
        },
        inactive: Budget {
            allocations: 349,
            reallocations: 9,
            bytes_allocated: 34_090,
            bytes_reallocated: 8_184,
        },
        plain_control: Budget {
            allocations: 350,
            reallocations: 16,
            bytes_allocated: 232_707,
            bytes_reallocated: 9_200,
        },
        slow_control: Budget {
            allocations: 371,
            reallocations: 25,
            bytes_allocated: 261_062,
            bytes_reallocated: 17_384,
        },
    },
    ScenarioBudget {
        line_count: 10_000,
        active: Budget {
            allocations: 371,
            reallocations: 37,
            bytes_allocated: 2_290_875,
            bytes_reallocated: 278_504,
        },
        inactive: Budget {
            allocations: 349,
            reallocations: 13,
            bytes_allocated: 156_970,
            bytes_reallocated: 131_064,
        },
        plain_control: Budget {
            allocations: 350,
            reallocations: 24,
            bytes_allocated: 2_139_051,
            bytes_reallocated: 147_440,
        },
        slow_control: Budget {
            allocations: 371,
            reallocations: 37,
            bytes_allocated: 2_290_286,
            bytes_reallocated: 278_504,
        },
    },
];

fn push_listing(document: &mut String, line_count: usize) {
    document.push_str("....\n");
    for line in 0..line_count {
        let _ = writeln!(
            document,
            "line {line:05}: fixed conditional allocation payload"
        );
    }
    document.push_str("....\n");
}

fn conditional_document(line_count: usize) -> String {
    let mut document = String::with_capacity(line_count * 52);
    document.push_str("ifdef::bench-active[]\n");
    push_listing(&mut document, line_count);
    document.push_str("endif::bench-active[]\n");
    document
}

fn plain_control_document(line_count: usize) -> String {
    let mut document = String::with_capacity(line_count * 52);
    push_listing(&mut document, line_count);
    document
}

fn slow_path_control_document(line_count: usize) -> String {
    let mut document = String::with_capacity(line_count * 52);
    document.push_str(":bench-note: fixed \\\ncontinuation\n\n");
    push_listing(&mut document, line_count);
    document
}

fn measure(input: &str, options: &Options) -> Result<Stats, acdc_parser::Error> {
    let region = Region::new(GLOBAL);
    let parsed = parse(black_box(input), black_box(options))?;
    black_box(&parsed);
    let stats = region.change();
    drop(parsed);
    Ok(stats)
}

fn assert_within_budget(case: &str, line_count: usize, stats: Stats, budget: Budget) {
    let allocated_byte_limit = budget
        .bytes_allocated
        .saturating_add(ALLOCATED_BYTE_LAYOUT_TOLERANCE);

    assert!(
        stats.allocations <= budget.allocations,
        "{case}/{line_count} allocation count regressed: {} > {} ({stats:?})",
        stats.allocations,
        budget.allocations
    );
    assert!(
        stats.reallocations <= budget.reallocations,
        "{case}/{line_count} reallocation count regressed: {} > {} ({stats:?})",
        stats.reallocations,
        budget.reallocations
    );
    assert!(
        stats.bytes_allocated <= allocated_byte_limit,
        "{case}/{line_count} allocated-byte count regressed: {} > {} ({stats:?})",
        stats.bytes_allocated,
        allocated_byte_limit
    );
    assert!(
        stats.bytes_reallocated <= budget.bytes_reallocated,
        "{case}/{line_count} reallocated-byte count regressed: {} > {} ({stats:?})",
        stats.bytes_reallocated,
        budget.bytes_reallocated
    );
}

#[test]
fn conditional_allocation_work_stays_within_budget() -> Result<(), acdc_parser::Error> {
    let active_options = Options::builder()
        .with_attribute("bench-active", true)
        .build();
    let inactive_options = Options::default();

    for budget in BUDGETS {
        let line_count = budget.line_count;
        let conditional = conditional_document(line_count);
        let plain_control = plain_control_document(line_count);
        let slow_control = slow_path_control_document(line_count);

        // Warm caches and one-time parser state before opening allocator regions.
        let _ = parse(&conditional, &active_options)?;
        let _ = parse(&conditional, &inactive_options)?;
        let _ = parse(&plain_control, &inactive_options)?;
        let _ = parse(&slow_control, &inactive_options)?;

        let active = measure(&conditional, &active_options)?;
        let inactive = measure(&conditional, &inactive_options)?;
        let plain_control = measure(&plain_control, &inactive_options)?;
        let slow_control = measure(&slow_control, &inactive_options)?;

        eprintln!(
            "conditional allocations/{line_count}: active={active:?} inactive={inactive:?} \
             plain_control={plain_control:?} slow_control={slow_control:?}"
        );

        assert_within_budget("active", line_count, active, budget.active);
        assert_within_budget("inactive", line_count, inactive, budget.inactive);
        assert_within_budget(
            "plain_control",
            line_count,
            plain_control,
            budget.plain_control,
        );
        assert_within_budget(
            "slow_control",
            line_count,
            slow_control,
            budget.slow_control,
        );
    }
    Ok(())
}
