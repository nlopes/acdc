use acdc_parser::*;
fn main() {
    println!("Block: {} bytes", std::mem::size_of::<Block>());
    println!("InlineNode: {} bytes", std::mem::size_of::<InlineNode>());
    println!("InlineMacro: {} bytes", std::mem::size_of::<InlineMacro>());
    println!("BlockMetadata: {} bytes", std::mem::size_of::<BlockMetadata>());
    println!("Location: {} bytes", std::mem::size_of::<Location>());
    println!("Plain: {} bytes", std::mem::size_of::<Plain>());
    println!("Bold: {} bytes", std::mem::size_of::<Bold>());
    println!("Paragraph: {} bytes", std::mem::size_of::<Paragraph>());
    println!("Title: {} bytes", std::mem::size_of::<Title>());
    println!("Anchor: {} bytes", std::mem::size_of::<Anchor>());
    println!("DelimitedBlock: {} bytes", std::mem::size_of::<DelimitedBlock>());
    println!("Section: {} bytes", std::mem::size_of::<Section>());
    println!("Option<String>: {} bytes", std::mem::size_of::<Option<String>>());
    println!("Vec<InlineNode>: {} bytes", std::mem::size_of::<Vec<InlineNode>>());
    println!("Vec<Block>: {} bytes", std::mem::size_of::<Vec<Block>>());
}
