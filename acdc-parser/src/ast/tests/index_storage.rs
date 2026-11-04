use crate::{InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("Plain label")]
#[case("*Plain label*")]
fn ordinary_index_labels_do_not_need_an_additional_catalog_snapshot(
    #[case] label: &str,
) -> Result<(), Error> {
    let parsed = parse(&format!("indexterm2:[{label}]"), &Options::default())?;
    let owner = parsed
        .document_storage()
        .blocks
        .iter()
        .find_map(|block| {
            if let crate::Block::Paragraph(paragraph) = block {
                paragraph.content.iter().find_map(|node| {
                    if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                        Some(term.as_ref())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or("index owner")?;
    assert!(std::ptr::eq(owner, owner.catalog_entry()));
    Ok(())
}
