#[derive(Debug)]
pub(crate) struct MarkupMatch {
    pub start: usize,
    pub end: usize,
    pub content: String,
}

/// Find the first constrained bold pattern (*text*) in the given text
pub(crate) fn find_constrained_bold_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if let Some(&ch) = chars.get(i) &&
             ch == '*' &&
                /* Check if this could be the start of a bold pattern */
                 let Some(m) = try_match_bold_at_position(&chars, i)
        {
            return Some(m);
        }
        i += 1;
    }

    None
}

/// Try to match a bold pattern starting at the given position
fn try_match_bold_at_position(chars: &[char], start: usize) -> Option<MarkupMatch> {
    let start_char = chars.get(start)?;
    if *start_char != '*' {
        return None;
    }

    // Check boundary condition: must be at start or preceded by whitespace or punctuation
    if start > 0 {
        let prev_char = *chars.get(start - 1)?;
        if !crate::grammar::document::match_constrained_boundary(
            u8::try_from(prev_char)
                .inspect_err(|e| {
                    tracing::error!(error=?e, "Failed to convert char to u8");
                })
                .ok()?,
        ) {
            return None;
        }
    }

    // Look for the closing *
    let mut i = start + 1;

    // Skip first character if it's not *, space, tab, or newline (constrained bold rule)
    if let Some(&ch) = chars.get(i)
        && matches!(ch, '*' | ' ' | '\t' | '\n')
    {
        return None; // Invalid constrained bold
    }

    // Find the content (everything up to the next *)
    let content_start = i;
    while let Some(&ch) = chars.get(i) {
        if ch == '*' {
            break;
        }
        i += 1;
    }

    if i >= chars.len() {
        return None; // No closing *
    }

    // Check boundary condition: closing * must be followed by whitespace, punctuation, or end
    if let Some(&next_char) = chars.get(i + 1)
        && !matches!(
            next_char,
            ' ' | '\t'
                | '\n'
                | ','
                | ';'
                | '"'
                | '.'
                | '?'
                | '!'
                | '<'
                | '>'
                | '/'
                | '-'
                | '|'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '\''
                | ':'
        )
    {
        return None;
    }

    let content: String = chars.get(content_start..i)?.iter().collect();
    if content.is_empty() {
        return None; // Empty bold text
    }

    Some(MarkupMatch {
        start,
        end: i + 1,
        content,
    })
}

/// Find the first unconstrained bold pattern (**text**) in the given text
pub(crate) fn find_unconstrained_bold_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len()
            && let (Some(&ch1), Some(&ch2)) = (chars.get(i), chars.get(i + 1))
            && ch1 == '*'
            && ch2 == '*'
        {
            let start = i;
            i += 2; // Skip the opening **

            // Find the closing **
            let content_start = i;
            while i + 1 < chars.len() {
                if let (Some(&close1), Some(&close2)) = (chars.get(i), chars.get(i + 1))
                    && close1 == '*'
                    && close2 == '*'
                {
                    // Found closing **
                    if let Some(content_slice) = chars.get(content_start..i) {
                        let content: String = content_slice.iter().collect();
                        if !content.is_empty() {
                            return Some(MarkupMatch {
                                start,
                                end: i + 2,
                                content,
                            });
                        }
                    }
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
    None
}

/// Find the first unconstrained italic pattern (__text__) in the given text
pub(crate) fn find_unconstrained_italic_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len()
            && let (Some(&ch1), Some(&ch2)) = (chars.get(i), chars.get(i + 1))
            && ch1 == '_'
            && ch2 == '_'
        {
            let start = i;
            i += 2; // Skip the opening __

            // Find the closing __
            let content_start = i;
            while i + 1 < chars.len() {
                if let (Some(&close1), Some(&close2)) = (chars.get(i), chars.get(i + 1))
                    && close1 == '_'
                    && close2 == '_'
                {
                    // Found closing __
                    if let Some(content_slice) = chars.get(content_start..i) {
                        let content: String = content_slice.iter().collect();
                        if !content.is_empty() {
                            return Some(MarkupMatch {
                                start,
                                end: i + 2,
                                content,
                            });
                        }
                    }
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
    None
}

/// Find the first italic pattern (_text_) in the given text
pub(crate) fn find_italic_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if let Some(&ch) = chars.get(i)
            && ch == '_'
        {
            let start = i;

            // Check boundary condition: _ must be preceded by whitespace, punctuation, or start
            if start > 0 {
                let prev_char = *chars.get(start - 1)?;
                if !crate::grammar::document::match_constrained_boundary(
                    u8::try_from(prev_char)
                        .inspect_err(|e| {
                            tracing::error!(error=?e, "Failed to convert char to u8");
                        })
                        .ok()?,
                ) {
                    i += 1;
                    continue;
                }
            }

            i += 1; // Skip the opening _
            let content_start = i;

            // Find the closing _
            while let Some(&curr_ch) = chars.get(i) {
                if curr_ch == '_' {
                    // Check boundary condition: closing _ must be followed by whitespace, punctuation, or end
                    if let Some(&next_char) = chars.get(i + 1)
                        && !matches!(
                            next_char,
                            ' ' | '\t'
                                | '\n'
                                | ','
                                | ';'
                                | '"'
                                | '.'
                                | '?'
                                | '!'
                                | '<'
                                | '>'
                                | '/'
                                | '-'
                                | '|'
                                | '('
                                | ')'
                                | '['
                                | ']'
                                | '{'
                                | '}'
                                | '\''
                                | ':'
                        )
                    {
                        i += 1;
                        continue;
                    }

                    let content: String = chars.get(content_start..i)?.iter().collect();
                    if !content.is_empty() {
                        return Some(MarkupMatch {
                            start,
                            end: i + 1,
                            content,
                        });
                    }
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
    None
}

/// Find the first superscript pattern (^text^) in the given text
/// Superscript text must be continuous (no spaces) and is unconstrained
pub(crate) fn find_superscript_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if let Some(&ch) = chars.get(i)
            && ch == '^'
        {
            // Skip if preceded by backslash (escaped)
            if i > 0 && chars.get(i - 1) == Some(&'\\') {
                i += 1;
                continue;
            }

            let start = i;
            i += 1; // Skip the opening ^
            let content_start = i;

            // Find the closing ^, ensuring content has no spaces
            while let Some(&curr_ch) = chars.get(i) {
                if curr_ch == '^' {
                    break;
                }
                // Reject if we find any whitespace (continuous text requirement)
                if curr_ch.is_whitespace() {
                    break;
                }
                i += 1;
            }

            // Check if we found a valid closing ^
            if let Some(&close_ch) = chars.get(i)
                && close_ch == '^'
            {
                let content: String = chars.get(content_start..i)?.iter().collect();
                if !content.is_empty() {
                    return Some(MarkupMatch {
                        start,
                        end: i + 1,
                        content,
                    });
                }
            }
        }
        i += 1;
    }
    None
}

/// Find the first subscript pattern (~text~) in the given text
/// Subscript text must be continuous (no spaces) and is unconstrained
pub(crate) fn find_subscript_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if let Some(&ch) = chars.get(i)
            && ch == '~'
        {
            // Skip if preceded by backslash (escaped)
            if i > 0 && chars.get(i - 1) == Some(&'\\') {
                i += 1;
                continue;
            }

            let start = i;
            i += 1; // Skip the opening ~
            let content_start = i;

            // Find the closing ~, ensuring content has no spaces
            while let Some(&curr_ch) = chars.get(i) {
                if curr_ch == '~' ||
                // Reject if we find any whitespace (continuous text requirement)
                 curr_ch.is_whitespace()
                {
                    break;
                }
                i += 1;
            }

            // Check if we found a valid closing ~
            if let Some(&close_ch) = chars.get(i)
                && close_ch == '~'
            {
                let content: String = chars.get(content_start..i)?.iter().collect();
                if !content.is_empty() {
                    return Some(MarkupMatch {
                        start,
                        end: i + 1,
                        content,
                    });
                }
            }
        }
        i += 1;
    }
    None
}

/// Find the first curved quotation pattern (`"text"`) in the given text
/// Curved quotation text must be continuous (no spaces) and is unconstrained
pub(crate) fn find_curved_quotation_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i + 1 < chars.len() {
        if let (Some(&ch1), Some(&ch2)) = (chars.get(i), chars.get(i + 1))
            && ch1 == '"'
            && ch2 == '`'
        {
            let start = i;
            i += 2; // Skip the opening "`
            let content_start = i;

            // Find the closing `", ensuring content has no spaces
            while i + 1 < chars.len() {
                if let (Some(&curr_ch), Some(&next_ch)) = (chars.get(i), chars.get(i + 1)) {
                    if curr_ch == '`' && next_ch == '"' {
                        break;
                    }
                    // Reject if we find any whitespace (continuous text requirement)
                    if curr_ch.is_whitespace() {
                        i = content_start;
                        break;
                    }
                }
                i += 1;
            }

            // Check if we found a valid closing `"
            if let (Some(&close1), Some(&close2)) = (chars.get(i), chars.get(i + 1))
                && close1 == '`'
                && close2 == '"'
            {
                let content: String = chars.get(content_start..i)?.iter().collect();
                if !content.is_empty() {
                    return Some(MarkupMatch {
                        start,
                        end: i + 2,
                        content,
                    });
                }
            }
        }
        i += 1;
    }
    None
}

/// Find the first curved apostrophe pattern (`'text'`) in the given text
/// Curved apostrophe text must be continuous (no spaces) and is unconstrained
pub(crate) fn find_curved_apostrophe_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i + 1 < chars.len() {
        if let (Some(&ch1), Some(&ch2)) = (chars.get(i), chars.get(i + 1))
            && ch1 == '\''
            && ch2 == '`'
        {
            let start = i;
            i += 2; // Skip the opening '`
            let content_start = i;

            // Find the closing `', ensuring content has no spaces
            while i + 1 < chars.len() {
                if let (Some(&curr_ch), Some(&next_ch)) = (chars.get(i), chars.get(i + 1)) {
                    if curr_ch == '`' && next_ch == '\'' {
                        break;
                    }
                    // Reject if we find any whitespace (continuous text requirement)
                    if curr_ch.is_whitespace() {
                        i = content_start;
                        break;
                    }
                }
                i += 1;
            }

            // Check if we found a valid closing `'
            if let (Some(&close1), Some(&close2)) = (chars.get(i), chars.get(i + 1))
                && close1 == '`'
                && close2 == '\''
            {
                let content: String = chars.get(content_start..i)?.iter().collect();
                if !content.is_empty() {
                    return Some(MarkupMatch {
                        start,
                        end: i + 2,
                        content,
                    });
                }
            }
        }
        i += 1;
    }
    None
}

/// Find the first constrained monospace pattern (`text`) in the given text
pub(crate) fn find_monospace_constrained_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if let Some(&ch) = chars.get(i)
            && ch == '`'
        {
            let start = i;

            // Check boundary condition: ` must be preceded by whitespace, punctuation, or start
            if start > 0 {
                let prev_char = *chars.get(start - 1)?;
                if !crate::grammar::document::match_constrained_boundary(
                    u8::try_from(prev_char)
                        .inspect_err(|e| {
                            tracing::error!(error=?e, "Failed to convert char to u8");
                        })
                        .ok()?,
                ) {
                    i += 1;
                    continue;
                }
            }

            i += 1; // Skip the opening `
            let content_start = i;

            // Find the closing `
            while let Some(&curr_ch) = chars.get(i) {
                if curr_ch == '`' {
                    // Check boundary condition: closing ` must be followed by whitespace, punctuation, or end
                    if let Some(&next_char) = chars.get(i + 1)
                        && !matches!(
                            next_char,
                            ' ' | '\t'
                                | '\n'
                                | ','
                                | ';'
                                | '"'
                                | '.'
                                | '?'
                                | '!'
                                | '<'
                                | '>'
                                | '/'
                                | '-'
                                | '|'
                                | '('
                                | ')'
                                | '['
                                | ']'
                                | '{'
                                | '}'
                                | '\''
                                | ':'
                        )
                    {
                        i += 1;
                        continue;
                    }

                    let content: String = chars.get(content_start..i)?.iter().collect();
                    if !content.is_empty() {
                        return Some(MarkupMatch {
                            start,
                            end: i + 1,
                            content,
                        });
                    }
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
    None
}

/// Find the first unconstrained monospace pattern (``text``) in the given text
pub(crate) fn find_monospace_unconstrained_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len()
            && let (Some(&ch1), Some(&ch2)) = (chars.get(i), chars.get(i + 1))
            && ch1 == '`'
            && ch2 == '`'
        {
            let start = i;
            i += 2; // Skip the opening ``
            let content_start = i;

            // Find the closing ``
            while i + 1 < chars.len() {
                if let (Some(&close1), Some(&close2)) = (chars.get(i), chars.get(i + 1))
                    && close1 == '`'
                    && close2 == '`'
                {
                    let content: String = chars.get(content_start..i)?.iter().collect();
                    if !content.is_empty() {
                        return Some(MarkupMatch {
                            start,
                            end: i + 2,
                            content,
                        });
                    }
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
    None
}

/// Find the first constrained highlight pattern (#text#) in the given text
pub(crate) fn find_highlight_constrained_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if let Some(&ch) = chars.get(i)
            && ch == '#'
        {
            let start = i;

            // Check boundary condition: # must be preceded by whitespace, punctuation, or start
            if start > 0 {
                let prev_char = *chars.get(start - 1)?;
                if !crate::grammar::document::match_constrained_boundary(
                    u8::try_from(prev_char)
                        .inspect_err(|e| {
                            tracing::error!(error=?e, "Failed to convert char to u8");
                        })
                        .ok()?,
                ) {
                    i += 1;
                    continue;
                }
            }

            i += 1; // Skip the opening #
            let content_start = i;

            // Find the closing #
            while let Some(&curr_ch) = chars.get(i) {
                if curr_ch == '#' {
                    // Check boundary condition: closing # must be followed by whitespace, punctuation, or end
                    if let Some(&next_char) = chars.get(i + 1)
                        && !matches!(
                            next_char,
                            ' ' | '\t'
                                | '\n'
                                | ','
                                | ';'
                                | '"'
                                | '.'
                                | '?'
                                | '!'
                                | '<'
                                | '>'
                                | '/'
                                | '-'
                                | '|'
                                | '('
                                | ')'
                                | '['
                                | ']'
                                | '{'
                                | '}'
                                | '\''
                                | ':'
                        )
                    {
                        i += 1;
                        continue;
                    }

                    let content: String = chars.get(content_start..i)?.iter().collect();
                    if !content.is_empty() {
                        return Some(MarkupMatch {
                            start,
                            end: i + 1,
                            content,
                        });
                    }
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
    None
}

/// Find the first unconstrained highlight pattern (##text##) in the given text
pub(crate) fn find_highlight_unconstrained_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len()
            && let (Some(&ch1), Some(&ch2)) = (chars.get(i), chars.get(i + 1))
            && ch1 == '#'
            && ch2 == '#'
        {
            let start = i;
            i += 2; // Skip the opening ##
            let content_start = i;

            // Find the closing ##
            while i + 1 < chars.len() {
                if let (Some(&close1), Some(&close2)) = (chars.get(i), chars.get(i + 1))
                    && close1 == '#'
                    && close2 == '#'
                {
                    let content: String = chars.get(content_start..i)?.iter().collect();
                    if !content.is_empty() {
                        return Some(MarkupMatch {
                            start,
                            end: i + 2,
                            content,
                        });
                    }
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constrained_bold_pattern_matching() {
        // Test constrained bold pattern
        let result = find_constrained_bold_pattern("This *text* is bold.");
        assert!(matches!(result, Some(r) if r.content == "text" && r.start == 5 && r.end == 11));

        // Test that invalid patterns are rejected
        let result = find_constrained_bold_pattern("This*text*is not.");
        assert!(result.is_none());
    }

    #[test]
    fn test_unconstrained_bold_pattern_matching() {
        // Test unconstrained bold pattern
        let result = find_unconstrained_bold_pattern("This **text** is bold.");
        assert!(matches!(result, Some(r) if r.content == "text" && r.start == 5 && r.end == 13));
    }

    #[test]
    fn test_italic_pattern_matching() {
        // Test italic pattern
        let result = find_italic_pattern("This _text_ is italic.");
        assert!(matches!(result, Some(r) if r.content == "text" && r.start == 5 && r.end == 11));
    }

    #[test]
    fn test_superscript_pattern_matching() {
        // Test superscript pattern
        let result = find_superscript_pattern("x^2^ is superscript.");
        assert!(matches!(result, Some(r) if r.content == "2" && r.start == 1 && r.end == 4));

        // Test that spaces are rejected
        let result = find_superscript_pattern("x^2 3^ is not superscript.");
        assert!(result.is_none());
    }

    #[test]
    fn test_subscript_pattern_matching() {
        // Test subscript pattern
        let result = find_subscript_pattern("H~2~O is water.");
        assert!(matches!(result, Some(r) if r.content == "2" && r.start == 1 && r.end == 4));

        // Test that spaces are rejected
        let result = find_subscript_pattern("H~2 3~O is not.");
        assert!(result.is_none());
    }

    #[test]
    fn test_curved_quotation_pattern_matching() {
        // Test curved quotation pattern
        let result = find_curved_quotation_pattern("Use \"`text`\" here.");
        assert!(matches!(result, Some(r) if r.content == "text" && r.start == 4 && r.end == 12));

        // Test that spaces are rejected
        let result = find_curved_quotation_pattern("Use \"`text with spaces`\" here.");
        assert!(result.is_none());
    }

    #[test]
    fn test_curved_apostrophe_pattern_matching() {
        // Test curved apostrophe pattern
        let result = find_curved_apostrophe_pattern("Use '`text`' here.");
        assert!(matches!(result, Some(r) if r.content == "text" && r.start == 4 && r.end == 12));

        // Test that spaces are rejected
        let result = find_curved_apostrophe_pattern("Use '`text with spaces`' here.");
        assert!(result.is_none());
    }

    #[test]
    fn test_constrained_bold_with_hyphens() {
        // Bug: attribute expansion produces *should-be-bold* but quotes parsing doesn't find it
        let result = find_constrained_bold_pattern("Value: *should-be-bold*");
        assert!(
            matches!(result, Some(ref r) if r.content == "should-be-bold"),
            "Expected to find bold pattern with hyphenated content, got: {result:?}"
        );
    }

    #[test]
    fn test_constrained_bold_at_end_of_string() {
        // Pattern at end of string (no trailing character)
        let result = find_constrained_bold_pattern("This is *bold*");
        assert!(
            matches!(result, Some(ref r) if r.content == "bold" && r.start == 8 && r.end == 14),
            "Expected to find bold at end of string, got: {result:?}"
        );
    }
}
