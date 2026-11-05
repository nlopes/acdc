//! Read nested elements and their attributes, then prepare an independent edit.

use acdc_parser::{
    Document, Options,
    ast::{BlockKind, DocumentRef, InlineKind, StructuralRef},
    parse,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parsed = parse(
        "= Library reads\n\n== Section\n\n[.lead]\nA *bold* word and image:photo.png[alt=Photo,title=\"\"].\n\n* First\n+\nAttached paragraph.\n\n|===\n| Left | Right\n|===\n",
        &Options::default(),
    )?;
    let document = parsed.document();

    for node in document.descendants() {
        let StructuralRef::Block(block) = node else {
            continue;
        };
        let position = block.location().start();
        println!("Block at {}:{}", position.line(), position.column());
        if let Some(attributes) = block.attributes() {
            for (name, value) in attributes.iter() {
                println!("  {name}: {value:?}");
            }
        }
        if let BlockKind::Paragraph(paragraph) = block.kind() {
            for inline in paragraph.content().inlines() {
                if let InlineKind::Image(image) = inline.kind() {
                    println!("  Image target: {:?}", image.source().kind());
                    println!("  Alt: {:?}; tooltip: {:?}", image.alt(), image.tooltip());
                }
            }
        }
    }

    for field in document.content_occurrences() {
        println!(
            "{:?}: {} inline nodes",
            field.role(),
            field.content().inlines().count()
        );
    }

    let mut edited = Document::from(document);
    let blocks = edited.take_blocks();
    edited.set_blocks(blocks.rev());
    println!(
        "Prepared copy: {} top-level blocks",
        DocumentRef::from(&edited).blocks().count()
    );
    Ok(())
}
