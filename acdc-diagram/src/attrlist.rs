//! Parsing the attribute list of a diagram block macro.
//!
//! A block style (`[graphviz,target,svg]`) reaches acdc already parsed, but a
//! block macro (`graphviz::chart.dot[format=svg,align=center]`) arrives as an
//! ordinary paragraph, because the parser has no registry of extension macro
//! names. The macro form is therefore recognised and split up here.

use std::collections::BTreeMap;

/// A parsed `name::target[attrlist]` block macro.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct MacroForm<'a> {
    /// The macro name, which is also the diagram type.
    pub(crate) name: &'a str,
    /// The target, before the attribute list.
    pub(crate) target: &'a str,
    /// Named attributes.
    pub(crate) attributes: BTreeMap<String, String>,
    /// Unnamed attributes, in source order.
    pub(crate) positional: Vec<String>,
    /// `%name` options.
    pub(crate) options: Vec<String>,
    /// `.name` roles.
    pub(crate) roles: Vec<String>,
    /// A `#name` identifier.
    pub(crate) id: Option<String>,
}

/// Recognise `name::target[attrlist]` on a line of its own.
///
/// `is_known` decides whether a name is a diagram type; anything else is left
/// alone so ordinary paragraphs that happen to contain `::` are untouched.
pub(crate) fn parse_macro<'a>(
    line: &'a str,
    is_known: &dyn Fn(&str) -> bool,
) -> Option<MacroForm<'a>> {
    let line = line.trim();
    let rest = line.strip_suffix(']')?;
    let (name, rest) = rest.split_once("::")?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    if !is_known(name) {
        return None;
    }
    // The target may itself contain brackets, so the attribute list starts at
    // the last `[` on the line.
    let open = rest.rfind('[')?;
    let (target, attrlist) = rest.split_at(open);

    let mut form = MacroForm {
        name,
        target: target.trim(),
        ..MacroForm::default()
    };
    apply_attrlist(&attrlist[1..], &mut form);
    Some(form)
}

/// Split an attribute list on its top-level commas and classify each entry.
fn apply_attrlist(attrlist: &str, form: &mut MacroForm<'_>) {
    for entry in split_entries(attrlist) {
        let entry = entry.trim();
        if entry.is_empty() {
            form.positional.push(String::new());
            continue;
        }
        match entry.split_once('=') {
            Some((name, value)) if is_attribute_name(name.trim()) => {
                form.attributes
                    .insert(name.trim().to_string(), unquote(value.trim()).to_string());
            }
            _ => apply_shorthand(entry, form),
        }
    }
}

/// Handle the `#id`, `.role` and `%option` shorthands, or record a positional.
fn apply_shorthand(entry: &str, form: &mut MacroForm<'_>) {
    let value = unquote(entry);
    if value.starts_with(['#', '.', '%']) {
        let mut rest = value;
        while let Some(marker) = rest.chars().next() {
            let body = &rest[marker.len_utf8()..];
            let end = body.find(['#', '.', '%']).unwrap_or(body.len());
            let (name, remainder) = body.split_at(end);
            match marker {
                '#' => form.id = Some(name.to_string()),
                '.' => form.roles.push(name.to_string()),
                _ => form.options.push(name.to_string()),
            }
            rest = remainder;
        }
        return;
    }
    form.positional.push(value.to_string());
}

/// Split on commas that are not inside quotes.
fn split_entries(attrlist: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut quote: Option<u8> = None;
    let mut start = 0;
    for (index, byte) in attrlist.bytes().enumerate() {
        // Inside quotes nothing separates; outside them, only a comma does.
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

/// Whether the text before an `=` reads as an attribute name rather than as
/// part of a positional value.
fn is_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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

    fn known(name: &str) -> bool {
        matches!(name, "graphviz" | "plantuml" | "graphviz_py")
    }

    fn parse(line: &str) -> Option<MacroForm<'_>> {
        parse_macro(line, &known)
    }

    #[test]
    fn parses_target_and_named_attributes() {
        let form = parse(r#"plantuml::activity.txt[format="svg", align="center"]"#).expect("macro");
        assert_eq!(form.name, "plantuml");
        assert_eq!(form.target, "activity.txt");
        assert_eq!(
            form.attributes.get("format").map(String::as_str),
            Some("svg")
        );
        assert_eq!(
            form.attributes.get("align").map(String::as_str),
            Some("center")
        );
    }

    #[test]
    fn keeps_positional_order() {
        let form = parse("graphviz::chart.dot[svg]").expect("macro");
        assert_eq!(form.positional, vec!["svg".to_string()]);
    }

    #[test]
    fn reads_shorthand_markers() {
        let form = parse("graphviz::chart.dot[#chart.big%interactive]").expect("macro");
        assert_eq!(form.id.as_deref(), Some("chart"));
        assert_eq!(form.roles, vec!["big".to_string()]);
        assert_eq!(form.options, vec!["interactive".to_string()]);
    }

    #[test]
    fn ignores_commas_inside_quotes() {
        let form = parse(r#"graphviz::c.dot[title="a, b", format=png]"#).expect("macro");
        assert_eq!(
            form.attributes.get("title").map(String::as_str),
            Some("a, b")
        );
        assert_eq!(
            form.attributes.get("format").map(String::as_str),
            Some("png")
        );
    }

    #[test]
    fn rejects_unknown_and_malformed_macros() {
        assert!(parse("image::photo.png[]").is_none());
        assert!(parse("see the graphviz::docs").is_none());
        assert!(parse("just a paragraph").is_none());
    }

    #[test]
    fn accepts_an_empty_attribute_list() {
        let form = parse("graphviz::chart.dot[]").expect("macro");
        assert_eq!(form.target, "chart.dot");
        assert!(form.attributes.is_empty());
    }
}
