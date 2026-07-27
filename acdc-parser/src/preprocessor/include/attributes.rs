#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttributeMissingPolicy {
    Drop,
    DropLine,
    Skip,
    Warn,
}

impl AttributeMissingPolicy {
    pub(super) fn from_attributes(attributes: &crate::DocumentAttributes<'_>) -> Self {
        match attributes.text("attribute-missing") {
            Some("drop") => Self::Drop,
            Some("drop-line") => Self::DropLine,
            Some("warn") => Self::Warn,
            Some(_) | None => Self::Skip,
        }
    }
}

pub(super) struct AttributeSubstitution {
    pub(super) value: String,
    pub(super) missing: Vec<String>,
}

pub(super) fn substitute_include_attributes(
    text: &str,
    attributes: &crate::DocumentAttributes<'_>,
    policy: AttributeMissingPolicy,
) -> AttributeSubstitution {
    let mut missing = Vec::new();
    let empty = crate::DocumentAttributeValue::from("");
    let mut value = crate::substitute_attributes(text, |name| {
        attributes.get(name).or_else(|| {
            missing.push(name.to_owned());
            matches!(
                policy,
                AttributeMissingPolicy::Drop | AttributeMissingPolicy::DropLine
            )
            .then_some(&empty)
        })
    })
    .into_owned();

    if policy == AttributeMissingPolicy::DropLine && !missing.is_empty() {
        value.clear();
    }
    AttributeSubstitution { value, missing }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ParsedAttribute {
    Named(String, String),
    Option(String),
}

fn split_attribute_fields(input: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;

    for (idx, ch) in input.char_indices() {
        if let Some(active_quote) = quote {
            if ch == active_quote && !escaped {
                quote = None;
            }
            escaped = ch == '\\' && !escaped;
            if ch != '\\' {
                escaped = false;
            }
        } else if matches!(ch, '"' | '\'') {
            quote = Some(ch);
            escaped = false;
        } else if ch == ',' {
            fields.push(&input[start..idx]);
            start = idx + ch.len_utf8();
        }
    }
    fields.push(&input[start..]);
    fields
}

fn is_attribute_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
        && chars.all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn parse_attribute_value(value: &str) -> String {
    let value = value.trim();
    let Some(quote) = value.chars().next().filter(|ch| matches!(ch, '"' | '\'')) else {
        return value.to_string();
    };
    let mut parsed = String::new();
    let mut chars = value[quote.len_utf8()..].chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&quote) {
            parsed.push(quote);
            chars.next();
        } else if ch == quote {
            return parsed;
        } else {
            parsed.push(ch);
        }
    }

    // Asciidoctor retains an unmatched opening quote as part of the value.
    format!("{quote}{parsed}")
}

pub(super) fn parse_include_attributes(input: &str) -> Vec<ParsedAttribute> {
    let mut parsed = Vec::new();
    for field in split_attribute_fields(input) {
        let field = field.trim();
        let Some((name, value)) = field.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !is_attribute_name(name) {
            continue;
        }
        let value = parse_attribute_value(value);
        if matches!(name, "options" | "opts") {
            parsed.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|option| !option.is_empty())
                    .map(|option| ParsedAttribute::Option(option.to_string())),
            );
        } else {
            parsed.push(ParsedAttribute::Named(name.to_string(), value));
        }
    }
    parsed
}
