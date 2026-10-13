use std::fmt;

use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_saphyr::{Span, Spanned};

#[derive(Clone, Debug)]
pub(super) enum Value {
    Null,
    Bool,
    Number(String, Option<NumberSource>),
    String(String),
    Sequence(Vec<Self>),
    Mapping(Vec<(String, Self)>),
}

#[derive(Clone, Debug)]
pub(super) struct NumberSource {
    span: Span,
    literal: Option<String>,
}

impl Value {
    pub(super) fn text(&self) -> Option<&str> {
        match self {
            Self::String(text) | Self::Number(text, _) => Some(text),
            Self::Null | Self::Bool | Self::Sequence(_) | Self::Mapping(_) => None,
        }
    }

    pub(super) fn retain_numeric_literals(&mut self, yaml: &str) {
        match self {
            Self::Number(_, Some(source)) => {
                if let Some(raw) = source
                    .span
                    .byte_offset()
                    .zip(source.span.byte_len())
                    .and_then(|(start, length)| {
                        usize::try_from(start)
                            .ok()
                            .zip(usize::try_from(length).ok())
                    })
                    .and_then(|(start, length)| {
                        start
                            .checked_add(length)
                            .and_then(|end| yaml.get(start..end))
                    })
                {
                    // RGB values such as 000000 lose their width when decoded as integers.
                    if raw.starts_with(|character: char| {
                        character.is_ascii_digit() || character == '-' || character == '+'
                    }) {
                        source.literal = Some(raw.to_owned());
                    }
                }
            }
            Self::Sequence(values) => {
                for value in values {
                    value.retain_numeric_literals(yaml);
                }
            }
            Self::Mapping(values) => {
                for (_, value) in values {
                    value.retain_numeric_literals(yaml);
                }
            }
            Self::Null | Self::Bool | Self::String(_) | Self::Number(_, None) => {}
        }
    }

    pub(super) fn colour_text(&self) -> Option<&str> {
        if let Self::Number(_, Some(source)) = self
            && let Some(literal) = &source.literal
        {
            Some(literal)
        } else {
            self.text()
        }
    }
}

pub(super) fn parse(yaml: &str) -> Result<Value, crate::Error> {
    let mut value: Value = serde_saphyr::from_str(yaml)?;
    value.retain_numeric_literals(yaml);
    Ok(value)
}

fn located(mut value: Spanned<Value>) -> Value {
    if let Value::Number(_, span) = &mut value.value {
        *span = Some(NumberSource {
            span: value.defined.span(),
            literal: None,
        });
    }
    value.value
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // References use values defined earlier in the YAML, including before an override.
        // Preserve mapping order rather than resolving against the final set of keys.
        deserializer.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML theme value")
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Value, E> {
        Ok(Value::Bool)
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.to_string(), None))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.to_string(), None))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
        Ok(Value::Number(value.to_string(), None))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<Spanned<Value>>()? {
            values.push(located(value));
        }
        Ok(Value::Sequence(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut mapping: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some((key, value)) = mapping.next_entry::<String, Spanned<Value>>()? {
            values.push((key, located(value)));
        }
        Ok(Value::Mapping(values))
    }
}
