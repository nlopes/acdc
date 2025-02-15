use pest::{iterators::Pairs, Parser as _};

use crate::{
    inline_preprocessing, DocumentAttributes, Error, InlinePreprocessorParserState, ListItem,
    Location, Rule,
};

impl ListItem {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<ListItem, Error> {
        let mut content = Vec::new();
        let mut level = 0;
        let mut marker = String::new();
        let mut checked = None;
        let mut location = Location::default();

        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.set_start_from_pos(&pair.as_span().start_pos());
            }
            if i == len - 1 {
                location.set_end_from_pos(&pair.as_span().end_pos());
            }
            match pair.as_rule() {
                Rule::list_item => {
                    let mut item_location = Location::from_pair(&pair);
                    item_location.shift(parent_location);

                    let text = pair.as_str();
                    let start_pos = pair.as_span().start_pos().pos();

                    // Run inline preprocessor before parsing inlines
                    let mut state = InlinePreprocessorParserState::new();
                    state.set_initial_position(&location, start_pos);
                    let processed = inline_preprocessing::run(text, parent_attributes, &state)
                        .map_err(|e| {
                            tracing::error!("error processing list item: {}", e);
                            Error::Parse(e.to_string())
                        })?;

                    // Now parse the processed text
                    let mut pairs = crate::InnerPestParser::parse(Rule::inlines, &processed.text)
                        .map_err(|e| Error::Parse(e.to_string()))?;

                    content.extend(crate::inlines::parse_inlines(
                        pairs.next().ok_or_else(|| {
                            tracing::error!("error parsing list item content");
                            Error::Parse("error parsing list item content".to_string())
                        })?,
                        Some(&processed),
                        Some(&item_location),
                        parent_attributes,
                    )?);
                }
                Rule::unordered_level | Rule::ordered_level => {
                    marker = pair.as_str().to_string();

                    level = u8::try_from(ListItem::parse_depth_from_marker(&marker).unwrap_or(1))
                        .map_err(|e| {
                        Error::Parse(format!("error with list level depth: {e}"))
                    })?;
                }
                Rule::checklist_item_checked => checked = Some(true),
                Rule::checklist_item_unchecked => checked = Some(false),
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        location.shift(parent_location);
        Ok(ListItem {
            level,
            marker,
            checked,
            content,
            location,
        })
    }

    /// This takes a single line and tries to parse it according to the described
    /// `AsciiDoc` list item rules. It simply identifies the depth of nesting. It handles
    /// the patterns as follows:
    ///
    /// Unordered list:
    /// * -> depth 1
    /// - -> depth 1
    ///
    /// ** -> depth 2
    ///
    /// Ordered list:
    /// . -> depth 1
    /// .. -> depth 2
    /// 1. , 10. -> depth 1 (numeric prefix with a dot)
    pub(crate) fn parse_depth_from_marker(marker: &str) -> Option<usize> {
        let trimmed = marker.trim();

        // Check for unordered lists first
        if trimmed.starts_with('*') {
            // Count how many '*' at the start
            let depth = trimmed.chars().take_while(|&c| c == '*').count();
            return Some(depth);
        }

        if trimmed.starts_with('-') {
            // '-' form only depth 1
            return Some(1);
        }

        // Check for ordered lists
        if trimmed.starts_with('.') {
            // Count how many '.' at the start
            let depth = trimmed.chars().take_while(|&c| c == '.').count();
            return Some(depth);
        }

        // Check if it starts with a digit followed by a dot
        // For example: "1. something" or "10. something"
        if let Some(dot_pos) = trimmed.find('.') {
            let (num_part, _) = trimmed.split_at(dot_pos);
            if num_part.chars().all(|c| c.is_ascii_digit()) {
                // It's a numeric ordered list at depth 1
                return Some(1);
            }
        }

        // If it doesn't match any known pattern
        None
    }
}
