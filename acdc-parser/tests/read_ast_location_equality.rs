use std::{alloc::System, hint::black_box, sync::Arc};

use acdc_parser::{
    Location, Position,
    ast::{LocationRef, PositionRef},
};
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

#[test]
fn source_position_equality_keeps_coordinates_and_include_chains_without_copying() {
    let make_position = || {
        let mut position = Position::new(3, 5);
        position.file = Some(Arc::new(vec!["outer.adoc".into(), "café.adoc".into()]));
        position
    };
    let original = make_position();
    let equal = make_position();
    let mutations: [fn(&mut Position); 6] = [
        |position| position.line += 1,
        |position| position.column += 1,
        |position| position.file = None,
        |position| position.file = Some(Arc::new(Vec::new())),
        |position| position.file = Some(Arc::new(vec!["outer.adoc".into(), "other.adoc".into()])),
        |position| position.file = Some(Arc::new(vec!["café.adoc".into(), "outer.adoc".into()])),
    ];
    let different: Vec<_> = mutations
        .into_iter()
        .map(|mutate| {
            let mut position = make_position();
            mutate(&mut position);
            position
        })
        .collect();
    let primary = Position::new(3, 5);
    let mut included_empty_chain = Position::new(3, 5);
    included_empty_chain.file = Some(Arc::new(Vec::new()));
    let region = Region::new(ALLOCATOR);
    let position = PositionRef::from(&original);
    assert_eq!(black_box(position), PositionRef::from(&equal));
    for other in &different {
        assert_ne!(black_box(position), PositionRef::from(other));
    }
    assert_ne!(
        PositionRef::from(&primary),
        PositionRef::from(&included_empty_chain)
    );
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
}
