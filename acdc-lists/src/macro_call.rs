//! Recognising the `list-of::` block macro.
//!
//! asciidoctor-lists registers `list-of` as a block macro, so Asciidoctor
//! hands the extension a parsed target and attribute list. acdc's parser has
//! no extension registry: an unknown block macro falls through to an ordinary
//! paragraph holding the macro text verbatim. The macro is therefore
//! recognised here, out of that paragraph, and its attribute list parsed.

use std::collections::BTreeMap;

/// The macro name, as written in a document.
const MACRO_NAME: &str = "list-of::";

/// A parsed `list-of::<element>[<attrlist>]` call.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ListMacro<'a> {
    /// The macro target: the element to list.
    pub(crate) target: &'a str,
    /// Named attributes from the attribute list.
    pub(crate) attributes: BTreeMap<&'a str, &'a str>,
}

impl<'a> ListMacro<'a> {
    /// Whether an attribute is set to a value `AsciiDoc` reads as true.
    ///
    /// asciidoctor-lists tests its flags for Ruby truthiness, where the mere
    /// presence of `hide_empty_section=` is enough. Writing `=false` is the
    /// obvious way to turn a flag back off, though, so that is honoured too.
    pub(crate) fn flag(&self, name: &str) -> bool {
        self.attributes
            .get(name)
            .is_some_and(|value| !matches!(*value, "false" | "0" | ""))
    }

    /// The element to list: the macro target, or the `element` attribute that
    /// the pre-1.0.6 `element_list::[element=image]` syntax used.
    pub(crate) fn element(&self) -> &'a str {
        if self.target.is_empty() {
            self.attributes.get("element").copied().unwrap_or_default()
        } else {
            self.target
        }
    }
}

/// Recognise `list-of::<element>[<attrlist>]` on a line of its own.
pub(crate) fn parse(line: &str) -> Option<ListMacro<'_>> {
    let line = line.trim();
    let rest = line.strip_prefix(MACRO_NAME)?;
    // The attribute list is required, as it is for every block macro, and runs
    // to the end of the line.
    let attrlist_start = rest.find('[')?;
    let rest = rest.strip_suffix(']')?;
    let (target, attrlist) = rest.split_at(attrlist_start);

    let mut call = ListMacro {
        target: target.trim(),
        ..ListMacro::default()
    };
    for entry in split_entries(&attrlist[1..]) {
        let Some((name, value)) = entry.split_once('=') else {
            continue;
        };
        call.attributes.insert(name.trim(), unquote(value.trim()));
    }
    Some(call)
}

/// Split an attribute list on the commas that are not inside quotes.
fn split_entries(attrlist: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut quote: Option<u8> = None;
    let mut start = 0;
    for (index, byte) in attrlist.bytes().enumerate() {
        if let Some(open) = quote {
            if byte == open {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b',' {
            entries.push(&attrlist[start..index]);
            start = index + 1;
        }
    }
    entries.push(&attrlist[start..]);
    entries
}

/// Strip one layer of matching quotes.
fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    match (bytes.first(), bytes.last()) {
        (Some(b'"'), Some(b'"')) | (Some(b'\''), Some(b'\'')) if value.len() >= 2 => {
            &value[1..value.len() - 1]
        }
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn reads_the_element_from_the_target() {
        let call = parse("list-of::image[]").expect("macro");
        assert_eq!(call.element(), "image");
        assert!(call.attributes.is_empty());
    }

    #[test]
    fn reads_named_attributes() {
        let call = parse("list-of::table[hide_empty_section=true,enhanced_rendering=true]")
            .expect("macro");
        assert_eq!(call.element(), "table");
        assert!(call.flag("hide_empty_section"));
        assert!(call.flag("enhanced_rendering"));
    }

    #[test]
    fn falls_back_to_the_element_attribute() {
        let call = parse("list-of::[element=listing]").expect("macro");
        assert_eq!(call.element(), "listing");
    }

    #[test]
    fn treats_an_explicit_false_as_unset() {
        let call = parse("list-of::image[hide_empty_section=false]").expect("macro");
        assert!(!call.flag("hide_empty_section"));
    }

    #[test]
    fn keeps_commas_inside_quotes_together() {
        let call = parse(r#"list-of::image[caption="a, b"]"#).expect("macro");
        assert_eq!(call.attributes.get("caption").copied(), Some("a, b"));
    }

    #[test]
    fn rejects_anything_that_is_not_the_macro() {
        assert!(parse("image::photo.png[]").is_none());
        assert!(parse("list-of::image").is_none());
        assert!(parse("see list-of::image[] here").is_none());
        assert!(parse("just a paragraph").is_none());
    }
}
