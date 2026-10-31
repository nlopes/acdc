use std::{alloc::System, hint::black_box, sync::Arc};

use acdc_parser::{Location, Position, ast::LocationRef};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn source_span_equality_retains_offsets_endpoints_and_include_chains_without_copying() {
    let make_span = || {
        let mut span = Location::point(Position::new(3, 5));
        span.absolute_start = 12;
        span.absolute_end = 20;
        span.end = Position::new(4, 7);
        span.start.file = Some(Arc::new(vec![
            String::from("outer.adoc"),
            String::from("a.adoc"),
        ]));
        span.end.file = Some(Arc::new(vec![
            String::from("outer.adoc"),
            String::from("b.adoc"),
        ]));
        span
    };
    let original = make_span();
    let equal = make_span();
    let mutations: [fn(&mut Location); 8] = [
        |span| span.absolute_start += 1,
        |span| span.absolute_end += 1,
        |span| span.start.line += 1,
        |span| span.start.column += 1,
        |span| span.end.line += 1,
        |span| span.end.column += 1,
        |span| span.start.file = None,
        |span| span.end.file = Some(Arc::new(vec![String::from("another.adoc")])),
    ];
    let different: Vec<_> = mutations
        .into_iter()
        .map(|mutate| {
            let mut span = make_span();
            mutate(&mut span);
            span
        })
        .collect();
    let region = Region::new(ALLOCATOR);
    let view = LocationRef::from(&original);
    assert_eq!(black_box(view), LocationRef::from(&equal));
    for span in &different {
        assert_ne!(black_box(view), LocationRef::from(span));
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
}
