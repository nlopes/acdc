use crate::{CharacterReference, CharacterReferenceKind, CompatibilityMode};

pub(super) fn body(
    text: &str,
    mode: CompatibilityMode,
) -> Option<(CharacterReferenceKind<'_>, usize)> {
    if let Some(number) = text.strip_prefix('#') {
        let (digits, radix, prefix) = number
            .strip_prefix('x')
            .map_or((number, 10, 1), |hex| (hex, 16, 2));
        let length = digits
            .bytes()
            .take_while(|byte| {
                if radix == 16 {
                    byte.is_ascii_hexdigit()
                } else {
                    byte.is_ascii_digit()
                }
            })
            .count();
        if length == 0 || digits.as_bytes().get(length) != Some(&b';') {
            return None;
        }
        if mode == CompatibilityMode::Asciidoctor
            && !(2..=if radix == 16 { 5 } else { 6 }).contains(&length)
        {
            return None;
        }
        let value = u32::from_str_radix(&digits[..length], radix).ok()?;
        return Some((CharacterReferenceKind::Numeric(value), prefix + length + 1));
    }
    let mut characters = text.char_indices();
    let (_, first) = characters.next()?;
    if !CharacterReference::name_start(first) {
        return None;
    }
    let mut end = first.len_utf8();
    for (position, character) in characters {
        if !CharacterReference::name_continue(character) {
            break;
        }
        end = position + character.len_utf8();
    }
    if text.as_bytes().get(end) != Some(&b';') {
        return None;
    }
    let name = &text[..end];
    if mode == CompatibilityMode::Asciidoctor {
        let letters = name.bytes().take_while(u8::is_ascii_alphabetic).count();
        let digits = &name[letters..];
        if letters < 2 || digits.len() > 2 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
    }
    Some((CharacterReferenceKind::Named(name), end + 1))
}

pub(super) fn whole(
    text: &str,
    mode: CompatibilityMode,
    location: crate::Location,
) -> Option<CharacterReference<'_>> {
    let (kind, length) = body(text.strip_prefix('&')?, mode)?;
    if length + 1 != text.len() {
        return None;
    }
    CharacterReference::new(kind, location)
}

/// Apply reference restoration to retained escaping state. Only authored
/// reference syntax is interpreted; output encoding belongs to converters.
pub(super) fn restore(
    text: &str,
    escaping: usize,
    mode: CompatibilityMode,
) -> (std::borrow::Cow<'_, str>, usize) {
    let (reference, escaped) = text.strip_prefix('\\').map_or((text, false), |s| (s, true));
    if escaping > 0 {
        if escaping > 1
            || reference
                .strip_prefix('&')
                .and_then(|s| body(s, mode))
                .is_some()
        {
            return (reference.into(), escaping - usize::from(!escaped));
        }
    } else if let Some(rest) = reference.strip_prefix("&amp;")
        && body(rest, mode).is_some()
    {
        return if escaped {
            (reference.into(), 0)
        } else {
            (format!("&{rest}").into(), 0)
        };
    }
    (text.into(), escaping)
}

/// Preserve an authored basic reference as literal text when it is part of a
/// larger token or protected from character-reference recognition.
pub(super) fn literal(text: &str, escaping: usize) -> (std::borrow::Cow<'_, str>, usize) {
    if escaping == 0 {
        for (reference, character) in [("&amp;", '&'), ("&lt;", '<'), ("&gt;", '>')] {
            if let Some(rest) = text.strip_prefix(reference) {
                return (format!("{character}{rest}").into(), 1);
            }
        }
    }
    (text.into(), escaping)
}
