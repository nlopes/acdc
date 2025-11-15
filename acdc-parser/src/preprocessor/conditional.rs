use std::path::Path;

use crate::{
    DocumentAttributes,
    error::{Error, Positioning, SourceLocation},
    model::{HEADER, Position, Substitute},
};

#[derive(Debug)]
pub(crate) enum Conditional {
    Ifdef(Ifdef),
    Ifndef(Ifndef),
    Ifeval(Ifeval),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Operation {
    Or,
    And,
}

#[derive(Debug)]
pub(crate) struct Ifdef {
    attributes: Vec<String>,
    content: Option<String>,
    operation: Option<Operation>,
}

#[derive(Debug)]
pub(crate) struct Ifndef {
    attributes: Vec<String>,
    content: Option<String>,
    operation: Option<Operation>,
}

#[derive(Debug)]
pub(crate) struct Ifeval {
    left: EvalValue,
    operator: Operator,
    right: EvalValue,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub(crate) enum EvalValue {
    String(String),
    Number(f64),
    Boolean(bool),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Operator {
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
}

#[derive(Debug)]
pub(crate) struct Endif {
    pub(crate) attribute: Option<String>,
}

peg::parser! {
    grammar conditional_parser() for str {
        pub(crate) rule conditional() -> Conditional
            = ifdef() / ifndef() / ifeval()

        pub(crate) rule endif() -> Endif
            = "endif::" attribute:name()? "[]" {
                Endif {
                    attribute
                }
            }

        rule ifdef() -> Conditional
            = "ifdef::" a:attributes() "[" content:content()? "]" {
                Conditional::Ifdef(Ifdef {
                    attributes: a.0,
                    operation: a.1,
                    content,
                })
            }

        rule ifndef() -> Conditional
            = "ifndef::" a:attributes() "[" content:content()? "]" {
                Conditional::Ifndef(Ifndef {
                    attributes: a.0,
                    operation: a.1,
                    content,
                })
            }

        rule ifeval() -> Conditional
            = "ifeval::[" left:eval_value() operator:operator() right:eval_value() "]" {

                // We parse everything we get here as a string, then whoever gets this,
                // should convert into the proper EvalValue
                Conditional::Ifeval(Ifeval {
                    left: EvalValue::String(left),
                    operator,
                    right: EvalValue::String(right)
                })
            }

        rule attributes() -> (Vec<String>, Option<Operation>)
            = n1:name() op:operation() rest:(n:name() { n })* {
                let mut names = vec![n1];
                names.extend(rest);
                (names, Some(op))
            }
        / n1:name() { (vec![n1], None) }

        rule operation() -> Operation
            = "+" { Operation::And }
        / "," { Operation::Or }

        rule eval_value() -> String
            = n:$((!operator() ![']'] [_])+)  {
                n.trim().to_string()
            }

        rule operator() -> Operator
        = "==" { Operator::Equal }
        / "!=" { Operator::NotEqual }
        / "<=" { Operator::LessThanOrEqual }
        / ">=" { Operator::GreaterThanOrEqual }
        / "<" { Operator::LessThan }
        / ">" { Operator::GreaterThan }

        rule name_match() = (!['[' | ',' | '+'] [_])+

        rule name() -> String
            = n:$(name_match())  {
                n.to_string()
            }

        rule content() -> String
            = c:$((!"]" [_])+) {
                c.to_string()
            }
    }
}

impl Conditional {
    fn evaluate_attributes(
        attrs: &[String],
        operation: Option<&Operation>,
        doc_attrs: &DocumentAttributes,
        negate: bool,
    ) -> bool {
        if attrs.is_empty() {
            tracing::warn!("no attributes in conditional directive but expecting at least one");
            return !negate; // ifdef: false, ifndef: true
        }

        let result = match operation {
            Some(Operation::Or) => attrs.iter().any(|attr| doc_attrs.contains_key(attr)),
            _ => attrs.iter().all(|attr| doc_attrs.contains_key(attr)),
        };

        if negate { !result } else { result }
    }

    pub(crate) fn is_true(
        &self,
        attributes: &DocumentAttributes,
        content: &mut String,
        line_number: usize,
        current_offset: usize,
        file_parent: Option<&Path>,
    ) -> Result<bool, Error> {
        Ok(match self {
            Conditional::Ifdef(ifdef) => {
                let is_true = Self::evaluate_attributes(
                    &ifdef.attributes,
                    ifdef.operation.as_ref(),
                    attributes,
                    false,
                );
                if is_true && let Some(if_content) = &ifdef.content {
                    content.clone_from(if_content);
                }
                is_true
            }
            Conditional::Ifndef(ifndef) => {
                let is_true = Self::evaluate_attributes(
                    &ifndef.attributes,
                    ifndef.operation.as_ref(),
                    attributes,
                    true,
                );
                if is_true && let Some(if_content) = &ifndef.content {
                    content.clone_from(if_content);
                }
                is_true
            }
            Conditional::Ifeval(ifeval) => {
                ifeval.evaluate(attributes, line_number, current_offset, file_parent)?
            }
        })
    }
}

impl Endif {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn closes(&self, conditional: &Conditional) -> bool {
        if let Some(attribute) = &self.attribute {
            match conditional {
                Conditional::Ifdef(ifdef) => ifdef.attributes.contains(attribute),
                Conditional::Ifndef(ifndef) => ifndef.attributes.contains(attribute),
                Conditional::Ifeval(_ifeval) => false,
            }
        } else {
            true
        }
    }
}

impl Ifeval {
    #[tracing::instrument(level = "trace", skip(file_parent))]
    fn evaluate(
        &self,
        attributes: &DocumentAttributes,
        line_number: usize,
        current_offset: usize,
        file_parent: Option<&Path>,
    ) -> Result<bool, Error> {
        let left = self.left.convert(attributes);
        let right = self.right.convert(attributes);

        // TOOD(nlopes): There are a few better ways to do this, but for now, this is
        // fine. I'm just going for functionality.
        match (&left, &right) {
            (EvalValue::Number(_), EvalValue::Number(_))
            | (EvalValue::Boolean(_), EvalValue::Boolean(_))
            | (EvalValue::String(_), EvalValue::String(_)) => {}
            _ => {
                tracing::error!("cannot compare different types of values in ifeval directive");
                return Err(Error::InvalidIfEvalDirectiveMismatchedTypes(Box::new(
                    SourceLocation {
                        file: file_parent.map(Path::to_path_buf),
                        positioning: Positioning::Position(Position {
                            line: line_number,
                            column: 1,
                            offset: current_offset,
                        }),
                    },
                )));
            }
        }

        Ok(match self.operator {
            Operator::Equal => left == right,
            Operator::NotEqual => left != right,
            Operator::LessThan => left < right,
            Operator::GreaterThan => left > right,
            Operator::LessThanOrEqual => left <= right,
            Operator::GreaterThanOrEqual => left >= right,
        })
    }
}

impl EvalValue {
    #[tracing::instrument(level = "trace")]
    fn convert(&self, attributes: &DocumentAttributes) -> Self {
        match self {
            EvalValue::String(s) => {
                // First we substitute any attributes in the string with their values
                let s = s.substitute(HEADER, attributes);

                // Try to parse as bool, f64, or evaluate as expression, otherwise return as string
                s.parse::<bool>()
                    .map(EvalValue::Boolean)
                    .or_else(|_| s.parse::<f64>().map(EvalValue::Number))
                    .or_else(|_| evalexpr::eval_float(&s).map(EvalValue::Number))
                    .or_else(|_| {
                        #[allow(clippy::cast_precision_loss)]
                        evalexpr::eval_int(&s)
                            .map(|v| v as f64)
                            .map(EvalValue::Number)
                    })
                    .unwrap_or_else(|_| EvalValue::String(Self::strip_quotes(&s)))
            }
            value @ (EvalValue::Number(_) | EvalValue::Boolean(_)) => value.clone(),
        }
    }

    #[tracing::instrument(level = "trace")]
    fn strip_quotes(s: &str) -> String {
        if s.starts_with('\'') && s.ends_with('\'') {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }
}

#[tracing::instrument(level = "trace", skip(file_parent))]
pub(crate) fn parse_line(
    line: &str,
    line_number: usize,
    current_offset: usize,
    file_parent: Option<&Path>,
) -> Result<Conditional, Error> {
    conditional_parser::conditional(line).map_err(|error| {
        tracing::error!(?error, "failed to parse conditional directive");
        Error::InvalidConditionalDirective(Box::new(SourceLocation {
            file: file_parent.map(Path::to_path_buf),
            positioning: Positioning::Position(Position {
                line: line_number,
                column: 1,
                offset: current_offset,
            }),
        }))
    })
}

#[tracing::instrument(level = "trace", skip(file_parent))]
pub(crate) fn parse_endif(
    line: &str,
    line_number: usize,
    current_offset: usize,
    file_parent: Option<&Path>,
) -> Result<Endif, Error> {
    conditional_parser::endif(line).map_err(|error| {
        tracing::error!(?error, "failed to parse endif directive");
        Error::InvalidConditionalDirective(Box::new(SourceLocation {
            file: file_parent.map(Path::to_path_buf),
            positioning: Positioning::Position(Position {
                line: line_number,
                column: 1,
                offset: current_offset,
            }),
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ifdef_single_attribute() -> Result<(), Error> {
        let line = "ifdef::attribute[]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional::Ifdef(ifdef) if ifdef.attributes == vec!["attribute"] && ifdef.operation.is_none() && ifdef.content.is_none())
        );
        Ok(())
    }

    #[test]
    fn test_ifdef_or_attributes() -> Result<(), Error> {
        let line = "ifdef::attr1,attr2[]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional::Ifdef(ifdef) if ifdef.attributes == vec!["attr1", "attr2"] && ifdef.operation == Some(Operation::Or) && ifdef.content.is_none())
        );
        Ok(())
    }

    #[test]
    fn test_ifdef_and_attributes() -> Result<(), Error> {
        let line = "ifdef::attr1+attr2[]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional::Ifdef(ifdef) if ifdef.attributes == vec!["attr1", "attr2"] && ifdef.operation == Some(Operation::And) && ifdef.content.is_none())
        );
        Ok(())
    }

    #[test]
    fn test_ifndef() -> Result<(), Error> {
        let line = "ifndef::attribute[]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional::Ifndef(ifndef) if ifndef.attributes == vec!["attribute"] && ifndef.operation.is_none() && ifndef.content.is_none())
        );
        Ok(())
    }

    #[test]
    fn test_ifeval_simple_math() -> Result<(), Error> {
        let line = "ifeval::[1 + 1 == 2]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(&conditional, Conditional::Ifeval(ifeval) if ifeval.left == EvalValue::String("1 + 1".to_string()) && ifeval.operator == Operator::Equal && ifeval.right == EvalValue::String("2".to_string()))
        );
        assert!(conditional.is_true(
            &DocumentAttributes::default(),
            &mut String::new(),
            1,
            0,
            None
        )?);
        Ok(())
    }

    #[test]
    fn test_ifeval_str_equality() -> Result<(), Error> {
        let line = "ifeval::['ASDF' == ASDF]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(&conditional, Conditional::Ifeval(ifeval) if ifeval.left == EvalValue::String("'ASDF'".to_string()) && ifeval.operator == Operator::Equal && ifeval.right == EvalValue::String("ASDF".to_string()))
        );
        assert!(conditional.is_true(
            &DocumentAttributes::default(),
            &mut String::new(),
            1,
            0,
            None
        )?);
        Ok(())
    }

    #[test]
    fn test_ifeval_greater_than_string_vs_number() -> Result<(), Error> {
        let line = "ifeval::['1+1' >= 2]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(&conditional, Conditional::Ifeval(ifeval) if ifeval.left == EvalValue::String("'1+1'".to_string()) && ifeval.operator == Operator::GreaterThanOrEqual && ifeval.right == EvalValue::String("2".to_string()))
        );

        assert!(matches!(
            conditional.is_true(
                &DocumentAttributes::default(),
                &mut String::new(),
                1,
                0,
                None
            ),
            Err(Error::InvalidIfEvalDirectiveMismatchedTypes(..))
        ));
        Ok(())
    }

    #[test]
    fn test_ifdef_with_content() -> Result<(), Error> {
        let line = "ifdef::attribute[Some content here]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional::Ifdef(ifdef) if ifdef.attributes == vec!["attribute"] && ifdef.operation.is_none() && ifdef.content == Some("Some content here".to_string()))
        );
        Ok(())
    }

    #[test]
    fn test_endif() -> Result<(), Error> {
        let line = "endif::attribute[]";
        let endif = parse_endif(line, 1, 0, None)?;
        assert_eq!(endif.attribute, Some("attribute".to_string()));
        Ok(())
    }

    #[test]
    fn test_endif_no_attribute() -> Result<(), Error> {
        let line = "endif::[]";
        let endif = parse_endif(line, 1, 0, None)?;
        assert_eq!(endif.attribute, None);
        Ok(())
    }
}
