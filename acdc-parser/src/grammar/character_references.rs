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
