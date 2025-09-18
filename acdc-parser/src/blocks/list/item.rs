use crate::ListItem;

impl ListItem {
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
