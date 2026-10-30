use std::{alloc::System, error::Error, hint::black_box};

use acdc_parser::{
    Block, Bold, Document, Form, InlineNode, Location, Options, Paragraph,
    ast::{BlockKind, BlockRef, DocumentRef, StructuralRef},
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, Stats, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn direct_field_reads_do_not_allocate() -> Result<(), Box<dyn Error>> {
    let parsed = parse(
        ":experimental:\n\n.Title\n[attribution='Credit',citetitle='Cite',caption='Prefix',reftext='Reference']\nimage:bird[Alt,title=Tip] kbd:[Ctrl+A] menu:File[Open > Recent] indexterm:[One,Two,Three,see-also=\"Four,Five\"]\n\n[[anchor,Label]]\nParagraph.\n",
        &Options::default(),
    )?;
    let block = parsed.document_ref().blocks().next().ok_or("block")?;
    let BlockKind::Paragraph(paragraph) = block.kind() else {
        return Err("paragraph".into());
    };
    let anchor_block = parsed
        .document_ref()
        .blocks()
        .nth(1)
        .ok_or("anchor block")?;
    let anchor = anchor_block.primary_anchor().ok_or("anchor")?;
    let region = Region::new(ALLOCATOR);
    for _ in 0..32 {
        for field in StructuralRef::from(block)
            .content_fields()
            .chain(anchor.content_fields())
        {
            black_box(field.owner());
            black_box(field.role());
            for inline in field.content().inlines() {
                black_box(inline);
            }
        }
        for inline in paragraph.content().inlines() {
            for field in inline.content_fields() {
                black_box(field);
                for child in field.content().inlines() {
                    black_box(child);
                }
            }
        }
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
    Ok(())
}

fn flat_document(width: usize) -> Document<'static> {
    let mut doc = Document::default();
    doc.blocks = (0..width)
        .map(|_| Block::Paragraph(Paragraph::new(Vec::new(), Location::default())))
        .collect();
    doc
}

fn measure(doc: &Document<'_>) -> (usize, Stats) {
    let region = Region::new(ALLOCATOR);
    let mut count = 0;
    for field in DocumentRef::from(doc).content_occurrences() {
        black_box(field);
        count += 1;
    }
    (count, region.change())
}

#[test]
fn traversal_storage_does_not_grow_with_sibling_count() {
    let narrow = flat_document(1);
    let wide = flat_document(4096);
    let (narrow_count, narrow_stats) = measure(&narrow);
    let (wide_count, wide_stats) = measure(&wide);
    assert_eq!(narrow_count, 1);
    assert_eq!(wide_count, 4096);
    assert_eq!(narrow_stats.allocations, wide_stats.allocations);
    assert_eq!(narrow_stats.reallocations, wide_stats.reallocations);
    assert_eq!(narrow_stats.bytes_allocated, wide_stats.bytes_allocated);
}

#[test]
fn deep_inline_walk_and_skipping_do_not_recurse() -> Result<(), Box<dyn Error>> {
    let result = std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let mut content = Vec::new();
            for _ in 0..4096 {
                content = vec![InlineNode::BoldText(Bold {
                    role: None,
                    id: None,
                    form: Form::Constrained,
                    content,
                    location: Location::default(),
                })];
            }
            let block = Block::Paragraph(Paragraph::new(content, Location::default()));
            let root = BlockRef::from(&block);
            let region = Region::new(ALLOCATOR);
            let mut skipped = root.content_occurrences();
            assert!(skipped.next().is_some());
            skipped.skip_descendants();
            assert!(skipped.next().is_none());
            drop(skipped);
            let skipped_stats = region.change();
            let region = Region::new(ALLOCATOR);
            let count = root.content_occurrences().count();
            let full_stats = region.change();
            assert_eq!(count, 4097);
            assert!(full_stats.bytes_allocated > skipped_stats.bytes_allocated);
            if let Block::Paragraph(paragraph) = block {
                let mut pending = paragraph.content;
                while let Some(inline) = pending.pop() {
                    if let InlineNode::BoldText(span) = inline {
                        pending.extend(span.content);
                    }
                }
            }
        })?
        .join();
    assert!(result.is_ok());
    Ok(())
}
