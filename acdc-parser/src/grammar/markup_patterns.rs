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
        if chars[i] == '*' {
            // Check if this could be the start of a bold pattern
            if let Some(m) = try_match_bold_at_position(&chars, i) {
                return Some(m);
            }
        }
        i += 1;
    }

    None
}

/// Try to match a bold pattern starting at the given position
fn try_match_bold_at_position(chars: &[char], start: usize) -> Option<MarkupMatch> {
    if start >= chars.len() || chars[start] != '*' {
        return None;
    }

    // Check boundary condition: must be at start or preceded by whitespace
    if start > 0 {
        let prev_char = chars[start - 1];
        if !matches!(prev_char, ' ' | '\t' | '\n' | '\r' | '<' | '>') {
            return None;
        }
    }

    // Look for the closing *
    let mut i = start + 1;

    // Skip first character if it's not *, space, tab, or newline (constrained bold rule)
    if i < chars.len() && matches!(chars[i], '*' | ' ' | '\t' | '\n') {
        return None; // Invalid constrained bold
    }

    // Find the content (everything up to the next *)
    let content_start = i;
    while i < chars.len() && chars[i] != '*' {
        i += 1;
    }

    if i >= chars.len() {
        return None; // No closing *
    }

    // Check boundary condition: closing * must be followed by whitespace, punctuation, or end
    if i + 1 < chars.len() {
        let next_char = chars[i + 1];
        if !matches!(
            next_char,
            ' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | '<' | '>'
        ) {
            return None;
        }
    }

    let content: String = chars[content_start..i].iter().collect();
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
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            let start = i;
            i += 2; // Skip the opening **

            // Find the closing **
            let content_start = i;
            while i + 1 < chars.len() {
                if chars[i] == '*' && chars[i + 1] == '*' {
                    // Found closing **
                    let content: String = chars[content_start..i].iter().collect();
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

/// Find the first unconstrained italic pattern (__text__) in the given text
pub(crate) fn find_unconstrained_italic_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '_' && chars[i + 1] == '_' {
            let start = i;
            i += 2; // Skip the opening __

            // Find the closing __
            let content_start = i;
            while i + 1 < chars.len() {
                if chars[i] == '_' && chars[i + 1] == '_' {
                    // Found closing __
                    let content: String = chars[content_start..i].iter().collect();
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

/// Find the first italic pattern (_text_) in the given text
pub(crate) fn find_italic_pattern(text: &str) -> Option<MarkupMatch> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '_' {
            let start = i;

            // Check boundary condition: _ must be preceded by whitespace, punctuation, or start
            if start > 0 {
                let prev_char = chars[start - 1];
                if !matches!(
                    prev_char,
                    ' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | '<' | '>'
                ) {
                    i += 1;
                    continue;
                }
            }

            i += 1; // Skip the opening _
            let content_start = i;

            // Find the closing _
            while i < chars.len() {
                if chars[i] == '_' {
                    // Check boundary condition: closing _ must be followed by whitespace, punctuation, or end
                    if i + 1 < chars.len() {
                        let next_char = chars[i + 1];
                        if !matches!(
                            next_char,
                            ' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | '<' | '>'
                        ) {
                            i += 1;
                            continue;
                        }
                    }

                    let content: String = chars[content_start..i].iter().collect();
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
        if chars[i] == '^' {
            let start = i;
            i += 1; // Skip the opening ^
            let content_start = i;

            // Find the closing ^, ensuring content has no spaces
            while i < chars.len() && chars[i] != '^' {
                // Reject if we find any whitespace (continuous text requirement)
                if chars[i].is_whitespace() {
                    break;
                }
                i += 1;
            }

            // Check if we found a valid closing ^
            if i < chars.len() && chars[i] == '^' {
                let content: String = chars[content_start..i].iter().collect();
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
        if chars[i] == '~' {
            let start = i;
            i += 1; // Skip the opening ~
            let content_start = i;

            // Find the closing ~, ensuring content has no spaces
            while i < chars.len() && chars[i] != '~' {
                // Reject if we find any whitespace (continuous text requirement)
                if chars[i].is_whitespace() {
                    break;
                }
                i += 1;
            }

            // Check if we found a valid closing ~
            if i < chars.len() && chars[i] == '~' {
                let content: String = chars[content_start..i].iter().collect();
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
        if chars[i] == '"' && chars[i + 1] == '`' {
            let start = i;
            i += 2; // Skip the opening "`
            let content_start = i;

            // Find the closing `", ensuring content has no spaces
            while i + 1 < chars.len() {
                if chars[i] == '`' && chars[i + 1] == '"' {
                    break;
                }
                // Reject if we find any whitespace (continuous text requirement)
                if chars[i].is_whitespace() {
                    i = content_start;
                    break;
                }
                i += 1;
            }

            // Check if we found a valid closing `"
            if i + 1 < chars.len() && chars[i] == '`' && chars[i + 1] == '"' {
                let content: String = chars[content_start..i].iter().collect();
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
        if chars[i] == '\'' && chars[i + 1] == '`' {
            let start = i;
            i += 2; // Skip the opening '`
            let content_start = i;

            // Find the closing `', ensuring content has no spaces
            while i + 1 < chars.len() {
                if chars[i] == '`' && chars[i + 1] == '\'' {
                    break;
                }
                // Reject if we find any whitespace (continuous text requirement)
                if chars[i].is_whitespace() {
                    i = content_start;
                    break;
                }
                i += 1;
            }

            // Check if we found a valid closing `'
            if i + 1 < chars.len() && chars[i] == '`' && chars[i + 1] == '\'' {
                let content: String = chars[content_start..i].iter().collect();
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
        if chars[i] == '`' {
            let start = i;

            // Check boundary condition: ` must be preceded by whitespace, punctuation, or start
            if start > 0 {
                let prev_char = chars[start - 1];
                if !matches!(
                    prev_char,
                    ' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | '<' | '>'
                ) {
                    i += 1;
                    continue;
                }
            }

            // Check if this might be a curved quote pattern - skip if so
            if i + 1 < chars.len()
                && (chars[i + 1] == '"' || chars[i + 1] == '\'') {
                    i += 1;
                    continue;
                }

            i += 1; // Skip the opening `
            let content_start = i;

            // Find the closing `
            while i < chars.len() {
                if chars[i] == '`' {
                    // Check boundary condition: closing ` must be followed by whitespace, punctuation, or end
                    if i + 1 < chars.len() {
                        let next_char = chars[i + 1];
                        if !matches!(
                            next_char,
                            ' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | '<' | '>'
                        ) {
                            i += 1;
                            continue;
                        }
                    }

                    let content: String = chars[content_start..i].iter().collect();
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
        if i + 1 < chars.len() && chars[i] == '`' && chars[i + 1] == '`' {
            let start = i;
            i += 2; // Skip the opening ``
            let content_start = i;

            // Find the closing ``
            while i + 1 < chars.len() {
                if chars[i] == '`' && chars[i + 1] == '`' {
                    let content: String = chars[content_start..i].iter().collect();
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
        if chars[i] == '#' {
            let start = i;

            // Check boundary condition: # must be preceded by whitespace, punctuation, or start
            if start > 0 {
                let prev_char = chars[start - 1];
                if !matches!(
                    prev_char,
                    ' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | '<' | '>'
                ) {
                    i += 1;
                    continue;
                }
            }

            i += 1; // Skip the opening #
            let content_start = i;

            // Find the closing #
            while i < chars.len() {
                if chars[i] == '#' {
                    // Check boundary condition: closing # must be followed by whitespace, punctuation, or end
                    if i + 1 < chars.len() {
                        let next_char = chars[i + 1];
                        if !matches!(
                            next_char,
                            ' ' | '\t' | '\n' | ',' | ';' | '"' | '.' | '?' | '!' | '<' | '>'
                        ) {
                            i += 1;
                            continue;
                        }
                    }

                    let content: String = chars[content_start..i].iter().collect();
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
        if i + 1 < chars.len() && chars[i] == '#' && chars[i + 1] == '#' {
            let start = i;
            i += 2; // Skip the opening ##
            let content_start = i;

            // Find the closing ##
            while i + 1 < chars.len() {
                if chars[i] == '#' && chars[i + 1] == '#' {
                    let content: String = chars[content_start..i].iter().collect();
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
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.start, 5);
        assert_eq!(m.end, 11);
        assert_eq!(m.content, "text");

        // Test that invalid patterns are rejected
        let result = find_constrained_bold_pattern("This*text*is not.");
        assert!(result.is_none());
    }

    #[test]
    fn test_unconstrained_bold_pattern_matching() {
        // Test unconstrained bold pattern
        let result = find_unconstrained_bold_pattern("This **text** is bold.");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.start, 5);
        assert_eq!(m.end, 13);
        assert_eq!(m.content, "text");
    }

    #[test]
    fn test_italic_pattern_matching() {
        // Test italic pattern
        let result = find_italic_pattern("This _text_ is italic.");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.start, 5);
        assert_eq!(m.end, 11);
        assert_eq!(m.content, "text");
    }

    #[test]
    fn test_superscript_pattern_matching() {
        // Test superscript pattern
        let result = find_superscript_pattern("x^2^ is superscript.");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.start, 1);
        assert_eq!(m.end, 4);
        assert_eq!(m.content, "2");

        // Test that spaces are rejected
        let result = find_superscript_pattern("x^2 3^ is not superscript.");
        assert!(result.is_none());
    }

    #[test]
    fn test_subscript_pattern_matching() {
        // Test subscript pattern
        let result = find_subscript_pattern("H~2~O is water.");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.start, 1);
        assert_eq!(m.end, 4);
        assert_eq!(m.content, "2");

        // Test that spaces are rejected
        let result = find_subscript_pattern("H~2 3~O is not.");
        assert!(result.is_none());
    }

    #[test]
    fn test_curved_quotation_pattern_matching() {
        // Test curved quotation pattern
        let result = find_curved_quotation_pattern("Use \"`text`\" here.");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.start, 4);
        assert_eq!(m.end, 12);
        assert_eq!(m.content, "text");

        // Test that spaces are rejected
        let result = find_curved_quotation_pattern("Use \"`text with spaces`\" here.");
        assert!(result.is_none());
    }

    #[test]
    fn test_curved_apostrophe_pattern_matching() {
        // Test curved apostrophe pattern
        let result = find_curved_apostrophe_pattern("Use '`text`' here.");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.start, 4);
        assert_eq!(m.end, 12);
        assert_eq!(m.content, "text");

        // Test that spaces are rejected
        let result = find_curved_apostrophe_pattern("Use '`text with spaces`' here.");
        assert!(result.is_none());
    }
}