use acdc_parser::*;

macro_rules! print_size {
    ($($t:ty),* $(,)?) => {
        $(println!("{:<30} {:>4} bytes", stringify!($t), std::mem::size_of::<$t>());)*
    };
}

fn main() {
    println!("{:<30} {:>10}", "Type", "Size");
    println!("{}", "-".repeat(42));

    print_size!(
        Block,
        InlineNode,
        InlineMacro,
        BlockMetadata,
        Location,
        Plain,
        Bold,
        Italic,
        Monospace,
        Highlight,
        Superscript,
        Subscript,
        Paragraph,
        Section,
        DelimitedBlock,
        Title,
        Anchor,
        Comment,
        ThematicBreak,
        PageBreak,
        DocumentAttribute,
    );

    println!("\n--- Composed types ---\n");
    print_size!(
        Option<String>,
        Option<Anchor>,
        Option<Location>,
        Vec<InlineNode>,
        Vec<Block>,
        Vec<String>,
    );

    println!("\n--- With boxing ---\n");
    print_size!(
        Box<DelimitedBlock>,
        Box<Section>,
        Box<Paragraph>,
        Box<InlineMacro>,
        Box<BlockMetadata>,
    );
}
