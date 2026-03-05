use std::borrow::Cow;

use unicode_width::UnicodeWidthChar;

/// Skip past an ANSI escape sequence (CSI or OSC) without collecting it.
///
/// The iterator must be positioned at the `\x1b` character. On return,
/// the iterator is advanced past the entire escape sequence.
fn skip_ansi_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    // consume '\x1b'
    chars.next();
    match chars.peek() {
        Some(&'[') => {
            // CSI sequence: \x1b[...letter
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        }
        Some(&']') => {
            // OSC sequence: \x1b]...ST
            chars.next();
            for c in chars.by_ref() {
                if c == '\x07' {
                    break;
                }
                if c == '\x1b' {
                    if chars.peek() == Some(&'\\') {
                        chars.next();
                    }
                    break;
                }
            }
        }
        _ => {}
    }
}

/// Consume an ANSI escape sequence starting at '\x1b' and return it as a string.
fn collect_ansi_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut seq = String::new();
    // consume '\x1b'
    if let Some(esc) = chars.next() {
        seq.push(esc);
    }
    match chars.peek() {
        Some(&'[') => {
            // CSI sequence: \x1b[...letter
            if let Some(bracket) = chars.next() {
                seq.push(bracket);
            }
            for c in chars.by_ref() {
                seq.push(c);
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        }
        Some(&']') => {
            // OSC sequence: \x1b]...ST
            if let Some(bracket) = chars.next() {
                seq.push(bracket);
            }
            while let Some(c) = chars.next() {
                seq.push(c);
                if c == '\x07' {
                    break;
                }
                if c == '\x1b' {
                    if let Some(&'\\') = chars.peek()
                        && let Some(backslash) = chars.next()
                    {
                        seq.push(backslash);
                    }
                    break;
                }
            }
        }
        _ => {}
    }
    seq
}

/// Calculate visible width of a string, skipping ANSI escape sequences.
///
/// Uses `unicode-width` to correctly measure CJK characters (width 2),
/// zero-width combining characters, and other non-ASCII text.
pub(crate) fn visible_len(s: &str) -> usize {
    let mut len = 0;
    let mut chars = s.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch == '\x1b' {
            skip_ansi_escape(&mut chars);
        } else {
            chars.next();
            len += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }
    len
}

/// Pad a string to target visible width with spaces.
pub(crate) fn pad_to_width(s: &str, target: usize) -> Cow<'_, str> {
    let visible = visible_len(s);
    if visible >= target {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(format!("{s}{}", " ".repeat(target - visible)))
    }
}

/// Word-wrap text that may contain ANSI escape sequences.
///
/// Wraps each logical line independently at word boundaries so that visible
/// width never exceeds `max_width`. ANSI state (bold, italic, color, etc.) is
/// tracked and re-applied on continuation lines so styling is preserved across
/// wraps.
///
/// Single words longer than `max_width` are placed on their own line without
/// breaking.
pub(crate) fn wrap_ansi_text(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return text.to_string();
    }

    let mut result = String::new();
    for (i, logical_line) in text.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }
        wrap_single_line(logical_line, max_width, &mut result);
    }
    result
}

/// Token produced by scanning a line for ANSI sequences and words.
enum Token {
    /// An ANSI escape sequence (CSI or OSC).
    AnsiEscape(String),
    /// A run of visible non-space characters.
    Word(String),
    /// One or more spaces.
    Space,
}

/// Tokenize a single line into ANSI escapes, words, and spaces.
fn tokenize(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    let mut current_word = String::new();

    while let Some(&ch) = chars.peek() {
        if ch == '\x1b' {
            // Flush any in-progress word
            if !current_word.is_empty() {
                tokens.push(Token::Word(std::mem::take(&mut current_word)));
            }
            // Collect the full escape sequence
            let seq = collect_ansi_escape(&mut chars);
            tokens.push(Token::AnsiEscape(seq));
        } else if ch == ' ' {
            // Flush any in-progress word
            if !current_word.is_empty() {
                tokens.push(Token::Word(std::mem::take(&mut current_word)));
            }
            // Consume consecutive spaces as a single Space token
            while chars.peek() == Some(&' ') {
                chars.next();
            }
            tokens.push(Token::Space);
        } else {
            chars.next();
            current_word.push(ch);
        }
    }

    if !current_word.is_empty() {
        tokens.push(Token::Word(current_word));
    }

    tokens
}

/// Return the SGR parameter strings that the given cancellation code cancels.
///
/// For example, `"22"` (bold off) cancels `"1"` (bold) and `"2"` (dim).
fn cancellation_targets(params: &str) -> &[&str] {
    match params {
        "22" => &["1", "2"],                                       // bold/dim off
        "23" => &["3"],                                            // italic off
        "24" => &["4"],                                            // underline off
        "25" => &["5", "6"],                                       // blink off
        "27" => &["7"],                                            // inverse off
        "28" => &["8"],                                            // hidden off
        "29" => &["9"],                                            // strikethrough off
        "39" => &["30", "31", "32", "33", "34", "35", "36", "37"], // default fg
        "49" => &["40", "41", "42", "43", "44", "45", "46", "47"], // default bg
        _ => &[],
    }
}

/// Returns true if `code` is a foreground color SGR (basic, 256-color, or truecolor).
fn is_foreground_color(code: &str) -> bool {
    if !code.starts_with("\x1b[") || !code.ends_with('m') {
        return false;
    }
    let params = &code[2..code.len() - 1];
    // Basic foreground: 30-37, 90-97
    // Extended: 38;5;N or 38;2;R;G;B
    params.starts_with("38;")
        || params
            .parse::<u32>()
            .is_ok_and(|n| (30..=37).contains(&n) || (90..=97).contains(&n))
}

/// Returns true if `code` is a background color SGR (basic, 256-color, or truecolor).
fn is_background_color(code: &str) -> bool {
    if !code.starts_with("\x1b[") || !code.ends_with('m') {
        return false;
    }
    let params = &code[2..code.len() - 1];
    // Basic background: 40-47, 100-107
    // Extended: 48;5;N or 48;2;R;G;B
    params.starts_with("48;")
        || params
            .parse::<u32>()
            .is_ok_and(|n| (40..=47).contains(&n) || (100..=107).contains(&n))
}

/// Enhanced `update_ansi_state` that also handles:
/// - `\x1b[39m` cancelling any foreground color (including `\x1b[38;5;Nm`)
/// - `\x1b[49m` cancelling any background color (including `\x1b[48;2;R;G;Bm`)
fn update_ansi_state_full(seq: &str, active_codes: &mut Vec<String>) {
    // Full reset
    if seq == "\x1b[0m" || seq == "\x1b[m" {
        active_codes.clear();
        return;
    }

    if !seq.starts_with("\x1b[") || !seq.ends_with('m') {
        return;
    }

    let params = &seq[2..seq.len() - 1];

    // Default foreground: remove all foreground colors
    if params == "39" {
        active_codes.retain(|c| !is_foreground_color(c));
        return;
    }

    // Default background: remove all background colors
    if params == "49" {
        active_codes.retain(|c| !is_background_color(c));
        return;
    }

    // Check simple cancellation targets
    let cancelled = cancellation_targets(params);
    if !cancelled.is_empty() {
        active_codes.retain(|code| {
            if !code.starts_with("\x1b[") || !code.ends_with('m') {
                return true;
            }
            let code_params = &code[2..code.len() - 1];
            !cancelled.contains(&code_params)
        });
        // Don't push cancellation codes
        return;
    }

    // New foreground color replaces old foreground
    if is_foreground_color(seq) {
        active_codes.retain(|c| !is_foreground_color(c));
    }

    // New background color replaces old background
    if is_background_color(seq) {
        active_codes.retain(|c| !is_background_color(c));
    }

    active_codes.push(seq.to_string());
}

/// Wrap a single logical line, appending the result to `out`.
fn wrap_single_line(line: &str, max_width: usize, out: &mut String) {
    let tokens = tokenize(line);
    let mut col: usize = 0;
    let mut active_codes: Vec<String> = Vec::new();
    let mut line_started = false;
    // Defer space emission so we don't leave trailing spaces before wraps
    let mut pending_space = false;

    for token in &tokens {
        match token {
            Token::AnsiEscape(seq) => {
                update_ansi_state_full(seq, &mut active_codes);
                out.push_str(seq);
            }
            Token::Space => {
                if line_started {
                    pending_space = true;
                }
            }
            Token::Word(word) => {
                let wlen = visible_len(word);
                let needed = if pending_space { 1 + wlen } else { wlen };

                if line_started && col + needed > max_width {
                    // Need to wrap: emit reset, newline, re-apply codes
                    if !active_codes.is_empty() {
                        out.push_str("\x1b[0m");
                    }
                    out.push('\n');
                    for code in &active_codes {
                        out.push_str(code);
                    }
                    out.push_str(word);
                    col = wlen;
                    pending_space = false;
                } else {
                    if pending_space {
                        out.push(' ');
                        col += 1;
                        pending_space = false;
                    }
                    out.push_str(word);
                    col += wlen;
                    line_started = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_wraps_at_word_boundaries() {
        let input = "hello world foo bar baz";
        let result = wrap_ansi_text(input, 12);
        assert_eq!(result, "hello world\nfoo bar baz");
    }

    #[test]
    fn no_wrapping_needed_for_short_text() {
        let input = "short";
        let result = wrap_ansi_text(input, 80);
        assert_eq!(result, "short");
    }

    #[test]
    fn ansi_bold_wraps_with_state_reapplied() {
        // "\x1b[1m" = bold on, "\x1b[0m" = reset
        let input = "\x1b[1mhello world foo bar\x1b[0m";
        let result = wrap_ansi_text(input, 12);
        // After wrapping, bold should be re-applied on the continuation line
        assert_eq!(result, "\x1b[1mhello world\x1b[0m\n\x1b[1mfoo bar\x1b[0m");
    }

    #[test]
    fn multiple_ansi_attributes_preserved() {
        // bold + italic
        let input = "\x1b[1m\x1b[3mhello world foo bar\x1b[0m";
        let result = wrap_ansi_text(input, 12);
        assert_eq!(
            result,
            "\x1b[1m\x1b[3mhello world\x1b[0m\n\x1b[1m\x1b[3mfoo bar\x1b[0m"
        );
    }

    #[test]
    fn ansi_reset_clears_active_state() {
        let input = "\x1b[1mbold\x1b[0m normal text here";
        let result = wrap_ansi_text(input, 12);
        // "bold normal" = 11 visible chars, fits. "text here" goes to next line.
        assert_eq!(result, "\x1b[1mbold\x1b[0m normal\ntext here");
    }

    #[test]
    fn long_single_word_not_broken() {
        let input = "supercalifragilisticexpialidocious";
        let result = wrap_ansi_text(input, 10);
        // Word is too long but should not be broken
        assert_eq!(result, "supercalifragilisticexpialidocious");
    }

    #[test]
    fn existing_newlines_preserved() {
        let input = "line one\nline two that is a bit longer than ten chars";
        let result = wrap_ansi_text(input, 15);
        assert_eq!(
            result,
            "line one\nline two that\nis a bit longer\nthan ten chars"
        );
    }

    #[test]
    fn empty_input() {
        assert_eq!(wrap_ansi_text("", 80), "");
    }

    #[test]
    fn osc_sequences_passed_through() {
        // OSC8 hyperlink: \x1b]8;;url\x1b\\text\x1b]8;;\x1b\\
        let input =
            "\x1b]8;;https://example.com\x1b\\click here\x1b]8;;\x1b\\ some more words to wrap";
        let result = wrap_ansi_text(input, 20);
        // "click here some more" = 20 visible, "words to wrap" wraps
        // OSC sequences should not count toward width
        assert_eq!(
            result,
            "\x1b]8;;https://example.com\x1b\\click here\x1b]8;;\x1b\\ some more\nwords to wrap"
        );
    }

    #[test]
    fn visible_len_skips_csi() {
        assert_eq!(visible_len("\x1b[1mhello\x1b[0m"), 5);
    }

    #[test]
    fn visible_len_skips_osc() {
        assert_eq!(
            visible_len("\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\"),
            4
        );
    }

    #[test]
    fn pad_to_width_adds_spaces() {
        assert_eq!(pad_to_width("hi", 5).as_ref(), "hi   ");
    }

    #[test]
    fn pad_to_width_no_op_when_wide_enough() {
        let result = pad_to_width("hello", 3);
        assert!(matches!(result, Cow::Borrowed("hello")));
    }

    #[test]
    fn zero_max_width_returns_unchanged() {
        let input = "some text";
        assert_eq!(wrap_ansi_text(input, 0), input);
    }

    // Unicode width tests

    #[test]
    fn visible_len_cjk_double_width() {
        // CJK characters are 2 columns wide
        assert_eq!(visible_len("你好"), 4);
        assert_eq!(visible_len("hello你好"), 9); // 5 + 4
    }

    #[test]
    fn visible_len_cjk_with_ansi() {
        assert_eq!(visible_len("\x1b[1m你好\x1b[0m"), 4);
    }

    #[test]
    fn wrap_cjk_text() {
        // Each CJK char is 2 wide, so "你好世界" = 8 cols
        let input = "你好 世界 测试";
        let result = wrap_ansi_text(input, 6);
        // "你好" = 4, + space + "世界" = 4 → 4+1+4=9 > 6, so wrap
        assert_eq!(result, "你好\n世界\n测试");
    }

    #[test]
    fn pad_to_width_cjk() {
        // "你好" = 4 visible width, pad to 6
        assert_eq!(pad_to_width("你好", 6).as_ref(), "你好  ");
    }

    // SGR cancellation tests

    #[test]
    fn sgr_bold_off_cancels_bold() {
        let mut codes = vec!["\x1b[1m".to_string()];
        update_ansi_state_full("\x1b[22m", &mut codes);
        assert!(codes.is_empty());
    }

    #[test]
    fn sgr_italic_off_cancels_italic() {
        let mut codes = vec!["\x1b[3m".to_string()];
        update_ansi_state_full("\x1b[23m", &mut codes);
        assert!(codes.is_empty());
    }

    #[test]
    fn sgr_underline_off_cancels_underline() {
        let mut codes = vec!["\x1b[4m".to_string()];
        update_ansi_state_full("\x1b[24m", &mut codes);
        assert!(codes.is_empty());
    }

    #[test]
    fn sgr_inverse_off_cancels_inverse() {
        let mut codes = vec!["\x1b[7m".to_string()];
        update_ansi_state_full("\x1b[27m", &mut codes);
        assert!(codes.is_empty());
    }

    #[test]
    fn sgr_default_fg_cancels_all_foreground() {
        let mut codes = vec![
            "\x1b[1m".to_string(),  // bold — should survive
            "\x1b[31m".to_string(), // red fg — should be removed
        ];
        update_ansi_state_full("\x1b[39m", &mut codes);
        assert_eq!(codes, vec!["\x1b[1m"]);
    }

    #[test]
    fn sgr_default_fg_cancels_extended_foreground() {
        let mut codes = vec!["\x1b[38;5;200m".to_string()];
        update_ansi_state_full("\x1b[39m", &mut codes);
        assert!(codes.is_empty());
    }

    #[test]
    fn sgr_default_bg_cancels_all_background() {
        let mut codes = vec![
            "\x1b[3m".to_string(),  // italic — should survive
            "\x1b[42m".to_string(), // green bg — should be removed
        ];
        update_ansi_state_full("\x1b[49m", &mut codes);
        assert_eq!(codes, vec!["\x1b[3m"]);
    }

    #[test]
    fn sgr_default_bg_cancels_extended_background() {
        let mut codes = vec!["\x1b[48;2;255;0;0m".to_string()];
        update_ansi_state_full("\x1b[49m", &mut codes);
        assert!(codes.is_empty());
    }

    #[test]
    fn sgr_new_fg_replaces_old_fg() {
        let mut codes = vec!["\x1b[31m".to_string()]; // red
        update_ansi_state_full("\x1b[32m", &mut codes); // green replaces red
        assert_eq!(codes, vec!["\x1b[32m"]);
    }

    #[test]
    fn sgr_new_bg_replaces_old_bg() {
        let mut codes = vec!["\x1b[41m".to_string()]; // red bg
        update_ansi_state_full("\x1b[42m", &mut codes); // green bg replaces red bg
        assert_eq!(codes, vec!["\x1b[42m"]);
    }

    #[test]
    fn sgr_cancellation_preserves_unrelated_codes() {
        let mut codes = vec![
            "\x1b[1m".to_string(), // bold
            "\x1b[3m".to_string(), // italic
            "\x1b[4m".to_string(), // underline
        ];
        update_ansi_state_full("\x1b[23m", &mut codes); // italic off
        assert_eq!(codes, vec!["\x1b[1m", "\x1b[4m"]);
    }

    #[test]
    fn bold_cancelled_mid_wrap_not_reapplied() {
        // bold on, some text, bold off, more text that wraps
        let input = "\x1b[1mbold\x1b[22m normal text that needs to wrap here";
        let result = wrap_ansi_text(input, 20);
        // After bold-off, no codes should be re-applied on wrap
        assert_eq!(
            result,
            "\x1b[1mbold\x1b[22m normal text\nthat needs to wrap\nhere"
        );
    }
}
