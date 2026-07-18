use std::path::Path;

use crate::{
    DocumentAttributes,
    error::{Error, SourceLocation},
    model::{HEADER, Position, substitute},
};

#[derive(Debug)]
pub(crate) struct Conditional<'input> {
    condition: Condition<'input>,
    content: Option<&'input str>,
}

#[derive(Debug)]
enum Condition<'input> {
    Ifdef(AttributeCondition<'input>),
    Ifndef(AttributeCondition<'input>),
    Ifeval(EvalCondition),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Operation {
    Or,
    And,
}

#[derive(Debug, PartialEq)]
struct AttributeCondition<'input> {
    attributes: Vec<&'input str>,
    operation: Option<Operation>,
}

#[derive(Debug)]
struct EvalCondition {
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
pub(crate) struct Endif<'input> {
    condition: Option<AttributeCondition<'input>>,
}

peg::parser! {
    grammar conditional_parser() for str {
        pub(crate) rule conditional() -> Conditional<'input>
            = ifdef() / ifndef() / ifeval()

        pub(crate) rule endif() -> Endif<'input>
            = "endif::" condition:attribute_condition()? "[]" {
                Endif {
                    condition
                }
            }

        rule ifdef() -> Conditional<'input>
            = "ifdef::" condition:attribute_condition() "[" content:content()? "]" {
                Conditional {
                    condition: Condition::Ifdef(condition),
                    content,
                }
            }

        rule ifndef() -> Conditional<'input>
            = "ifndef::" condition:attribute_condition() "[" content:content()? "]" {
                Conditional {
                    condition: Condition::Ifndef(condition),
                    content,
                }
            }

        rule ifeval() -> Conditional<'input>
            = "ifeval::[" left:eval_value() operator:operator() right:eval_value() "]" {

                // We parse everything we get here as a string, then whoever gets this,
                // should convert into the proper EvalValue
                Conditional {
                    condition: Condition::Ifeval(EvalCondition {
                        left: EvalValue::String(left),
                        operator,
                        right: EvalValue::String(right)
                    }),
                    content: None,
                }
            }

        rule attribute_condition() -> AttributeCondition<'input>
            = first:name() rest:("," name:name() { name })+ {
                let mut attributes = Vec::with_capacity(rest.len() + 1);
                attributes.push(first);
                attributes.extend(rest);
                AttributeCondition::new(attributes, Some(Operation::Or))
            }
        / first:name() rest:("+" name:name() { name })+ {
                let mut attributes = Vec::with_capacity(rest.len() + 1);
                attributes.push(first);
                attributes.extend(rest);
                AttributeCondition::new(attributes, Some(Operation::And))
            }
        / name:name() { AttributeCondition::new(vec![name], None) }

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

        rule name() -> &'input str
            = n:$(name_match())  {
                n
            }

        rule content() -> &'input str
            = c:$((!"]" [_])+) {
                c
            }
    }
}

impl<'input> AttributeCondition<'input> {
    fn new(attributes: Vec<&'input str>, operation: Option<Operation>) -> Self {
        Self {
            attributes,
            operation,
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.operation == other.operation
            && self.attributes.len() == other.attributes.len()
            && self
                .attributes
                .iter()
                .zip(&other.attributes)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
    }
}

impl Conditional<'_> {
    fn evaluate_attributes(
        attrs: &[&str],
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
        let is_true = match &self.condition {
            Condition::Ifdef(condition) => Self::evaluate_attributes(
                &condition.attributes,
                condition.operation.as_ref(),
                attributes,
                false,
            ),
            Condition::Ifndef(condition) => Self::evaluate_attributes(
                &condition.attributes,
                condition.operation.as_ref(),
                attributes,
                true,
            ),
            Condition::Ifeval(ifeval) => {
                ifeval.evaluate(attributes, line_number, current_offset, file_parent)?
            }
        };
        if is_true && let Some(if_content) = &self.content {
            content.push_str(if_content);
        }
        Ok(is_true)
    }

    pub(crate) fn has_inline_content(&self) -> bool {
        self.content.is_some()
    }

    fn attribute_condition(&self) -> Option<&AttributeCondition<'_>> {
        match &self.condition {
            Condition::Ifdef(condition) | Condition::Ifndef(condition) => Some(condition),
            Condition::Ifeval(_) => None,
        }
    }
}

impl Endif<'_> {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn closes(&self, conditional: &Conditional<'_>) -> bool {
        match (&self.condition, conditional.attribute_condition()) {
            (None, _) => true,
            (Some(endif), Some(opening)) => endif.matches(opening),
            (Some(_), None) => false,
        }
    }
}

impl EvalCondition {
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
                        location: crate::Location::point(Position::from_line_col(line_number, 1)),
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
                let s = substitute(s, HEADER, attributes);

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
pub(crate) fn parse_line<'input>(
    line: &'input str,
    line_number: usize,
    current_offset: usize,
    file_parent: Option<&Path>,
) -> Result<Conditional<'input>, Error> {
    conditional_parser::conditional(line).map_err(|error| {
        tracing::error!(?error, "failed to parse conditional directive");
        Error::InvalidConditionalDirective(Box::new(SourceLocation {
            file: file_parent.map(Path::to_path_buf),
            location: crate::Location::point(Position::from_line_col(line_number, 1)),
        }))
    })
}

#[tracing::instrument(level = "trace", skip(file_parent))]
pub(crate) fn parse_endif<'input>(
    line: &'input str,
    line_number: usize,
    current_offset: usize,
    file_parent: Option<&Path>,
) -> Result<Endif<'input>, Error> {
    conditional_parser::endif(line).map_err(|error| {
        tracing::error!(?error, "failed to parse endif directive");
        Error::InvalidConditionalDirective(Box::new(SourceLocation {
            file: file_parent.map(Path::to_path_buf),
            location: crate::Location::point(Position::from_line_col(line_number, 1)),
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
            matches!(conditional, Conditional { condition: Condition::Ifdef(condition), content: None } if condition.attributes == vec!["attribute"] && condition.operation.is_none())
        );
        Ok(())
    }

    #[test]
    fn test_ifdef_or_attributes() -> Result<(), Error> {
        let line = "ifdef::attr1,attr2[]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional { condition: Condition::Ifdef(condition), content: None } if condition.attributes == vec!["attr1", "attr2"] && condition.operation == Some(Operation::Or))
        );
        Ok(())
    }

    #[test]
    fn test_ifdef_and_attributes() -> Result<(), Error> {
        let line = "ifdef::attr1+attr2[]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional { condition: Condition::Ifdef(condition), content: None } if condition.attributes == vec!["attr1", "attr2"] && condition.operation == Some(Operation::And))
        );
        Ok(())
    }

    #[test]
    fn test_ifdef_three_or_attributes() -> Result<(), Error> {
        let conditional = parse_line("ifdef::attr1,attr2,attr3[]", 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional { condition: Condition::Ifdef(condition), .. } if condition.attributes == vec!["attr1", "attr2", "attr3"] && condition.operation == Some(Operation::Or))
        );
        Ok(())
    }

    #[test]
    fn test_ifdef_three_and_attributes() -> Result<(), Error> {
        let conditional = parse_line("ifdef::attr1+attr2+attr3[]", 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional { condition: Condition::Ifdef(condition), .. } if condition.attributes == vec!["attr1", "attr2", "attr3"] && condition.operation == Some(Operation::And))
        );
        Ok(())
    }

    #[test]
    fn test_ifdef_rejects_mixed_attribute_operators() {
        assert!(matches!(
            parse_line("ifdef::attr1,attr2+attr3[]", 1, 0, None),
            Err(Error::InvalidConditionalDirective(..))
        ));
    }

    #[test]
    fn test_ifndef() -> Result<(), Error> {
        let line = "ifndef::attribute[]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(conditional, Conditional { condition: Condition::Ifndef(condition), content: None } if condition.attributes == vec!["attribute"] && condition.operation.is_none())
        );
        Ok(())
    }

    #[test]
    fn test_ifeval_simple_math() -> Result<(), Error> {
        let line = "ifeval::[1 + 1 == 2]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(&conditional, Conditional { condition: Condition::Ifeval(ifeval), content: None } if ifeval.left == EvalValue::String("1 + 1".to_string()) && ifeval.operator == Operator::Equal && ifeval.right == EvalValue::String("2".to_string()))
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
            matches!(&conditional, Conditional { condition: Condition::Ifeval(ifeval), content: None } if ifeval.left == EvalValue::String("'ASDF'".to_string()) && ifeval.operator == Operator::Equal && ifeval.right == EvalValue::String("ASDF".to_string()))
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
            matches!(&conditional, Conditional { condition: Condition::Ifeval(ifeval), content: None } if ifeval.left == EvalValue::String("'1+1'".to_string()) && ifeval.operator == Operator::GreaterThanOrEqual && ifeval.right == EvalValue::String("2".to_string()))
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
            matches!(conditional, Conditional { condition: Condition::Ifdef(condition), content: Some(content) } if condition.attributes == vec!["attribute"] && condition.operation.is_none() && content == "Some content here")
        );
        Ok(())
    }

    #[test]
    fn test_endif() -> Result<(), Error> {
        let line = "endif::attribute[]";
        let endif = parse_endif(line, 1, 0, None)?;
        assert!(matches!(
            endif.condition,
            Some(AttributeCondition {
                attributes,
                operation: None,
            }) if attributes == ["attribute"]
        ));
        Ok(())
    }

    #[test]
    fn test_endif_no_attribute() -> Result<(), Error> {
        let line = "endif::[]";
        let endif = parse_endif(line, 1, 0, None)?;
        assert_eq!(endif.condition, None);
        Ok(())
    }

    #[test]
    fn test_endif_matches_complete_condition_case_insensitively() -> Result<(), Error> {
        let conditional = parse_line("ifdef::Backend-PDF,Backend-DocBook5[]", 1, 0, None)?;
        let endif = parse_endif("endif::backend-pdf,backend-docbook5[]", 2, 0, None)?;
        assert!(endif.closes(&conditional));
        Ok(())
    }

    #[test]
    fn test_endif_rejects_partial_or_reordered_condition() -> Result<(), Error> {
        let conditional = parse_line("ifdef::backend-pdf,backend-docbook5[]", 1, 0, None)?;
        let partial = parse_endif("endif::backend-pdf[]", 2, 0, None)?;
        let reordered = parse_endif("endif::backend-docbook5,backend-pdf[]", 2, 0, None)?;
        let different_operation = parse_endif("endif::backend-pdf+backend-docbook5[]", 2, 0, None)?;
        assert!(!partial.closes(&conditional));
        assert!(!reordered.closes(&conditional));
        assert!(!different_operation.closes(&conditional));
        Ok(())
    }
}
