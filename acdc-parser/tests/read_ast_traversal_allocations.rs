use std::{alloc::System, hint::black_box};

use acdc_parser::{
    Block, DelimitedBlock, DelimitedBlockType, Document, Location, Paragraph,
    ast::{BlockRef, DocumentRef},
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, Stats, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn paragraph() -> Block<'static> {
    Block::Paragraph(Paragraph::new(Vec::new(), Location::default()))
}

fn tree(width: usize, depth: usize) -> Block<'static> {
    let mut block = Block::DelimitedBlock(DelimitedBlock::new(
        DelimitedBlockType::DelimitedOpen((0..width).map(|_| paragraph()).collect()),
        "--",
        Location::default(),
    ));
    for _ in 0..depth {
        block = Block::DelimitedBlock(DelimitedBlock::new(
            DelimitedBlockType::DelimitedOpen(vec![block, paragraph()]),
            "--",
            Location::default(),
        ));
    }
    block
}

#[test]
fn direct_children_and_flat_walks_do_not_allocate() {
    let mut document = Document::default();
    document.blocks = (0..4096).map(|_| paragraph()).collect();
    let root = DocumentRef::from(&document);
    let region = Region::new(ALLOCATOR);
    let mut children = 0;
    for node in root.children() {
        black_box(node);
        children += 1;
    }
    let mut descendants = 0;
    for node in root.descendants() {
        black_box(node);
        descendants += 1;
    }
    let stats = region.change();
    assert_eq!((children, descendants), (4096, 4096));
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
}

fn measure(block: &Block<'_>) -> (usize, Stats) {
    let region = Region::new(ALLOCATOR);
    let mut count = 0;
    for node in BlockRef::from(block).descendants() {
        black_box(node);
        count += 1;
    }
    (count, region.change())
}

#[test]
fn nested_walk_scratch_depends_on_depth_not_width() {
    let narrow = tree(1, 40);
    let wide = tree(4096, 40);
    let (narrow_count, narrow_stats) = measure(&narrow);
    let (wide_count, wide_stats) = measure(&wide);
    assert_eq!((narrow_count, wide_count), (81, 4176));
    assert!(narrow_stats.allocations > 0);
    assert_eq!(narrow_stats.allocations, wide_stats.allocations);
    assert_eq!(narrow_stats.reallocations, wide_stats.reallocations);
    assert_eq!(narrow_stats.bytes_allocated, wide_stats.bytes_allocated);
    assert_eq!(narrow_stats.bytes_reallocated, wide_stats.bytes_reallocated);
}

#[test]
fn stopping_before_descent_does_not_allocate_pending_stack_frames() {
    let owner = tree(4096, 40);
    let region = Region::new(ALLOCATOR);
    let count = BlockRef::from(&owner).descendants().take(1).count();
    let stats = region.change();
    assert_eq!(count, 1);
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
}
