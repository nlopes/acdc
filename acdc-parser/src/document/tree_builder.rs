use tracing::instrument;

use crate::{Block, DelimitedBlock, DelimitedBlockType, DiscreteHeader, Error, ErrorDetail};

#[instrument(level = "trace")]
fn build_section_tree_delimited(block: Block, kept_layers: &mut Vec<Block>) -> Result<(), Error> {
    if let Block::DelimitedBlock(delimited_block) = block {
        match &delimited_block.inner {
            // TODO(nlopes): I need to figure out why I had added this code here
            //
            // If I remove the DelimitedSidebar from the match, the discrete_header test fails
            //
            // DelimitedBlockType::DelimitedExample(blocks) => {
            //     let mut blocks = blocks.clone();
            //     build_section_tree(&mut blocks)?;
            //     kept_layers.push(Block::DelimitedBlock(DelimitedBlock {
            //         metadata: delimited_block.metadata,
            //         inner: DelimitedBlockType::DelimitedExample(blocks),
            //         title: delimited_block.title,
            //         delimiter: delimited_block.delimiter,
            //         location: delimited_block.location,
            //     }));
            // }
            // DelimitedBlockType::DelimitedQuote(blocks) => {
            //     let mut blocks = blocks.clone();
            //     build_section_tree(&mut blocks)?;
            //     kept_layers.push(Block::DelimitedBlock(DelimitedBlock {
            //         metadata: delimited_block.metadata,
            //         inner: DelimitedBlockType::DelimitedQuote(blocks),
            //         title: delimited_block.title,
            //         delimiter: delimited_block.delimiter,
            //         location: delimited_block.location,
            //     }));
            // }
            // DelimitedBlockType::DelimitedOpen(blocks) => {
            //     let mut blocks = blocks.clone();
            //     build_section_tree(&mut blocks)?;
            //     kept_layers.push(Block::DelimitedBlock(DelimitedBlock {
            //         metadata: delimited_block.metadata,
            //         inner: DelimitedBlockType::DelimitedOpen(blocks),
            //         title: delimited_block.title,
            //         delimiter: delimited_block.delimiter,
            //         location: delimited_block.location,
            //     }));
            // }
            DelimitedBlockType::DelimitedSidebar(blocks) => {
                let mut blocks = blocks.clone();
                build_section_tree(&mut blocks)?;
                kept_layers.push(Block::DelimitedBlock(DelimitedBlock {
                    metadata: delimited_block.metadata,
                    inner: DelimitedBlockType::DelimitedSidebar(blocks),
                    title: delimited_block.title,
                    delimiter: delimited_block.delimiter,
                    location: delimited_block.location,
                }));
            }
            _ => {
                kept_layers.push(Block::DelimitedBlock(delimited_block));
            }
        }
    } else {
        tracing::error!("expected a delimited block");
        return Err(Error::UnexpectedBlock(block.to_string()));
    }
    Ok(())
}

// Build a tree of sections from the content blocks.
#[instrument(level = "trace")]
pub(crate) fn build_section_tree(document: &mut Vec<Block>) -> Result<(), Error> {
    let mut current_layers = document.clone();
    let mut stack: Vec<Block> = Vec::new();

    current_layers.reverse();

    let mut kept_layers = Vec::new();
    for block in current_layers.drain(..) {
        match (block, stack.is_empty()) {
            (delimited_block @ Block::DelimitedBlock(_), true) => {
                build_section_tree_delimited(delimited_block, &mut kept_layers)?;
            }
            (Block::_DiscreteHeaderSection(section), true) => {
                stack.extend(section.content);
                stack.push(Block::DiscreteHeader(DiscreteHeader {
                    anchors: section.anchors,
                    title: section.title,
                    level: section.level,
                    location: section.location,
                }));
            }
            (Block::Section(section), true) => {
                kept_layers.push(Block::Section(section));
            }
            (Block::Section(section), false) => {
                let mut section = section;
                while let Some(block_from_stack) = stack.pop() {
                    section.location.end = match &block_from_stack {
                        Block::_DiscreteHeaderSection(section) => section.location.end.clone(),
                        Block::Section(section) => section.location.end.clone(),
                        Block::DelimitedBlock(delimited_block) => {
                            delimited_block.location.end.clone()
                        }
                        Block::Paragraph(paragraph) => paragraph.location.end.clone(),
                        Block::OrderedList(ordered_list) => ordered_list.location.end.clone(),
                        Block::UnorderedList(unordered_list) => unordered_list.location.end.clone(),
                        Block::DocumentAttribute(attribute) => attribute.location.end.clone(),
                        Block::Admonition(admonition) => admonition.location.end.clone(),
                        Block::TableOfContents(toc) => toc.location.end.clone(),
                        Block::ThematicBreak(thematic_break) => thematic_break.location.end.clone(),
                        Block::PageBreak(page_break) => page_break.location.end.clone(),
                        Block::DescriptionList(description_list) => {
                            description_list.location.end.clone()
                        }
                        Block::Image(image) => image.location.end.clone(),
                        Block::Audio(audio) => audio.location.end.clone(),
                        Block::Video(video) => video.location.end.clone(),
                        Block::DiscreteHeader(discrete_header) => {
                            discrete_header.location.end.clone()
                        }
                    };
                    section.content.push(block_from_stack);
                }
                kept_layers.push(Block::Section(section));
            }
            (block, _) => {
                stack.push(block);
            }
        }
    }

    stack.reverse();

    // Add the remaining blocks to the kept_layers
    while let Some(block_from_stack) = stack.pop() {
        kept_layers.push(block_from_stack);
    }

    if !kept_layers.is_empty() {
        let mut i = 0;
        while i < kept_layers.len() - 1 {
            let should_move = {
                if let (Some(Block::Section(section)), Some(Block::Section(next_section))) =
                    (kept_layers.get(i), kept_layers.get(i + 1))
                {
                    // TODO(nlopes): this if here is probably wrong - I added it because I
                    // was tired of debugging but this smells like a bug.
                    if section.level == 0 {
                        false
                    } else {
                        match next_section.level.cmp(&(section.level - 1)) {
                            std::cmp::Ordering::Greater => false,
                            std::cmp::Ordering::Equal => true,
                            std::cmp::Ordering::Less => {
                                let error_detail = ErrorDetail {
                                    location: next_section.location.clone(),
                                };
                                return Err(Error::NestedSectionLevelMismatch(
                                    error_detail,
                                    section.level - 1,
                                    section.level,
                                ));
                            }
                        }
                    }
                } else {
                    false
                }
            };

            if should_move {
                section_tree_move(&mut kept_layers, i)?;
            } else {
                i += 1;
            }
        }
        kept_layers.reverse();
    }
    *document = kept_layers;
    Ok(())
}

#[instrument(level = "trace")]
fn section_tree_move(kept_layers: &mut Vec<Block>, i: usize) -> Result<(), Error> {
    if let Some(Block::Section(current_section)) = kept_layers.get(i).cloned() {
        if let Some(Block::Section(parent_section)) = kept_layers.get_mut(i + 1) {
            parent_section.location.end = match &current_section.content.last() {
                Some(Block::Section(section)) => section.location.end.clone(),
                Some(Block::DelimitedBlock(delimited_block)) => {
                    delimited_block.location.end.clone()
                }
                Some(Block::Paragraph(paragraph)) => paragraph.location.end.clone(),
                _ => todo!(),
            };
            parent_section.content.push(Block::Section(current_section));
            kept_layers.remove(i);
        } else {
            return Err(Error::Parse("expected a section".to_string()));
        }
    }
    Ok(())
}
