use tracing::instrument;

use crate::{Block, Error, ErrorDetail};

// Validate that the block level is correct for the section level.
//
// For example, a section level 1 should only contain blocks of level 2 or higher.
#[instrument(level = "trace")]
pub(crate) fn section_block_level(content: &[Block], prior_level: Option<u8>) -> Result<(), Error> {
    let mut prior_level = prior_level;
    for (i, block) in content.iter().enumerate() {
        if let Block::Section(section) = block {
            if let Some(Block::Section(next_section)) = content.get(i + 1)
                && next_section.level > section.level + 1
            {
                let error_detail = ErrorDetail {
                    location: next_section.location.clone(),
                };
                return Err(Error::NestedSectionLevelMismatch(
                    error_detail,
                    section.level,
                    section.level + 1,
                ));
            }
            if let Some(parent_level) = prior_level {
                if section.level == parent_level + 1 {
                    prior_level = Some(section.level);
                    return section_block_level(&section.content, prior_level);
                }
                let error_detail = ErrorDetail {
                    location: section.location.clone(),
                };
                return Err(Error::NestedSectionLevelMismatch(
                    error_detail,
                    section.level,
                    parent_level + 1,
                ));
            }
            prior_level = Some(section.level);
            return section_block_level(&section.content, prior_level);
        }
    }
    Ok(())
}
