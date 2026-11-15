mod peg;

use peg::conditional_parser;

use std::{cmp::Ordering, path::Path};

use crate::{
    DocumentAttributes, Location,
    error::{Error, SourceLocation},
    model::{HEADER, Position, apply_attribute_substitutions},
};

#[derive(Debug)]
pub(super) struct Conditional<'input> {
    condition: Condition<'input>,
    content: Option<&'input str>,
}

#[derive(Debug)]
enum Condition<'input> {
    Ifdef(AttributeCondition<'input>),
    Ifndef(AttributeCondition<'input>),
    Ifeval(EvalCondition),
    Unevaluable(Option<AttributeCondition<'input>>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Operation {
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
enum EvalValue {
    String(String),
    Number(Number),
    Boolean(bool),
}

/// Integers stay exact instead of rounding to `f64`, so values beyond 2^53
/// still compare correctly, as with Asciidoctor's arbitrary-precision integers.
#[derive(Debug, Clone, Copy)]
enum Number {
    Integer(i128),
    Float(f64),
}

#[derive(Debug, PartialEq)]
enum Operator {
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
}

#[derive(Debug)]
pub(super) struct Endif<'input> {
    condition: Option<AttributeCondition<'input>>,
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

impl<'input> Conditional<'input> {
    /// An invalid opening still owns its block. Keep a recognizable target so
    /// a named terminator can close it, but never expose an unevaluated body.
    pub(super) fn unevaluable(line: &'input str) -> Self {
        let body = line.split_once("::").map_or("", |(_, body)| body);
        let (target, content) = body.split_once('[').unwrap_or((body, ""));
        let inline = !line.starts_with("ifeval::")
            && content
                .strip_suffix(']')
                .is_some_and(|text| !text.is_empty());
        Self {
            condition: Condition::Unevaluable(conditional_parser::attribute_condition(target).ok()),
            content: inline.then_some(""),
        }
    }

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

    pub(super) fn is_true(
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
            Condition::Unevaluable(_) => false,
        };
        if is_true && let Some(if_content) = &self.content {
            content.push_str(if_content);
        }
        Ok(is_true)
    }

    pub(super) fn has_inline_content(&self) -> bool {
        self.content.is_some()
    }

    fn attribute_condition(&self) -> Option<&AttributeCondition<'_>> {
        match &self.condition {
            Condition::Ifdef(condition) | Condition::Ifndef(condition) => Some(condition),
            Condition::Ifeval(_) => None,
            Condition::Unevaluable(condition) => condition.as_ref(),
        }
    }
}

impl Endif<'_> {
    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) fn closes(&self, conditional: &Conditional<'_>) -> bool {
        match (&self.condition, conditional.attribute_condition()) {
            (None, _) => true,
            (Some(endif), Some(opening)) => endif.matches(opening),
            (Some(_), None) => false,
        }
    }
}

impl EvalCondition {
    #[tracing::instrument(level = "trace", skip_all, fields(line_number = line_number, current_offset = current_offset))]
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
                return Err(Error::InvalidIfEvalDirectiveMismatchedTypes(Box::new(
                    SourceLocation {
                        file: file_parent.map(Path::to_path_buf),
                        location: Location::point(Position::from_line_col(line_number, 1)),
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
    #[tracing::instrument(level = "trace", skip_all)]
    fn convert(&self, attributes: &DocumentAttributes) -> Self {
        match self {
            EvalValue::String(s) => {
                // First we substitute any attributes in the string with their values
                let s = apply_attribute_substitutions(s, HEADER, attributes);

                // Quotes select string comparisons even for numbers and booleans.
                if let Some(value) = Self::strip_quotes(&s) {
                    return EvalValue::String(value.to_string());
                }

                if let Ok(value) = s.parse::<bool>() {
                    return EvalValue::Boolean(value);
                }
                Number::parse(&s)
                    .map_or_else(|| EvalValue::String(s.into_owned()), EvalValue::Number)
            }
            value @ (EvalValue::Number(_) | EvalValue::Boolean(_)) => value.clone(),
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn strip_quotes(s: &str) -> Option<&str> {
        s.strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
            .or_else(|| s.strip_prefix('"')?.strip_suffix('"'))
    }
}

impl Number {
    /// 2^127 is exactly representable and is the first value above `i128::MAX`.
    const I128_BOUND: f64 = 170_141_183_460_469_231_731_687_303_715_884_105_728.0;

    /// Integer text stays an exact integer; other numbers parse as `f64`.
    /// Arithmetic isn't evaluated, so an operand such as `1 + 1` stays text.
    fn parse(text: &str) -> Option<Self> {
        text.parse()
            .map(Self::Integer)
            .or_else(|_| text.parse().map(Self::Float))
            .ok()
    }

    /// Compares exactly instead of rounding the integer to the nearest `f64`.
    fn compare_integer_to_float(integer: i128, float: f64) -> Option<Ordering> {
        if float.is_nan() {
            None
        } else if float >= Self::I128_BOUND {
            Some(Ordering::Less)
        } else if float < -Self::I128_BOUND {
            Some(Ordering::Greater)
        } else {
            // The truncated value is an integer within range, so the cast is exact.
            #[allow(clippy::cast_possible_truncation)]
            let whole = float.trunc() as i128;
            let ordering = integer.cmp(&whole);
            if ordering.is_eq() {
                0.0_f64.partial_cmp(&float.fract())
            } else {
                Some(ordering)
            }
        }
    }
}

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (*self, *other) {
            (Self::Integer(left), Self::Integer(right)) => Some(left.cmp(&right)),
            (Self::Float(left), Self::Float(right)) => left.partial_cmp(&right),
            (Self::Integer(left), Self::Float(right)) => {
                Self::compare_integer_to_float(left, right)
            }
            (Self::Float(left), Self::Integer(right)) => {
                Self::compare_integer_to_float(right, left).map(Ordering::reverse)
            }
        }
    }
}

#[tracing::instrument(level = "trace", skip_all, fields(line_number = line_number, current_offset = current_offset))]
pub(super) fn parse_line<'input>(
    line: &'input str,
    line_number: usize,
    current_offset: usize,
    file_parent: Option<&Path>,
) -> Result<Conditional<'input>, Error> {
    conditional_parser::conditional(line).map_err(|_| {
        Error::InvalidConditionalDirective(Box::new(SourceLocation {
            file: file_parent.map(Path::to_path_buf),
            location: Location::point(Position::from_line_col(line_number, 1)),
        }))
    })
}

#[tracing::instrument(level = "trace", skip_all, fields(line_number = line_number, current_offset = current_offset))]
pub(super) fn parse_endif<'input>(
    line: &'input str,
    line_number: usize,
    current_offset: usize,
    file_parent: Option<&Path>,
) -> Result<Endif<'input>, Error> {
    conditional_parser::endif(line).map_err(|_| {
        Error::InvalidConditionalDirective(Box::new(SourceLocation {
            file: file_parent.map(Path::to_path_buf),
            location: Location::point(Position::from_line_col(line_number, 1)),
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
    fn effective_defaults_are_defined_for_conditionals() -> Result<(), Error> {
        let attributes = DocumentAttributes::default();
        let defined = parse_line("ifdef::max-include-depth[]", 1, 0, None)?;
        let undefined = parse_line("ifndef::max-include-depth[]", 1, 0, None)?;

        assert!(defined.is_true(&attributes, &mut String::new(), 1, 0, None)?);
        assert!(!undefined.is_true(&attributes, &mut String::new(), 1, 0, None)?);
        Ok(())
    }

    #[test]
    fn ifeval_uses_typed_default_text_presentation() -> Result<(), Error> {
        let conditional = parse_line("ifeval::[{max-include-depth} == 64]", 1, 0, None)?;

        assert!(conditional.is_true(
            &DocumentAttributes::default(),
            &mut String::new(),
            1,
            0,
            None
        )?);

        let mut attributes = DocumentAttributes::default();
        assert!(
            attributes
                .set("max-include-depth".into(), "03".into())
                .is_ok()
        );
        let explicit_condition = parse_line("ifeval::[{max-include-depth} == 3]", 1, 0, None)?;
        assert!(explicit_condition.is_true(&attributes, &mut String::new(), 1, 0, None)?);
        Ok(())
    }

    #[test]
    fn ifeval_does_not_evaluate_arithmetic() -> Result<(), Error> {
        let line = "ifeval::[1 + 1 == 2]";
        let conditional = parse_line(line, 1, 0, None)?;
        assert!(
            matches!(&conditional, Conditional { condition: Condition::Ifeval(ifeval), content: None } if ifeval.left == EvalValue::String("1 + 1".to_string()) && ifeval.operator == Operator::Equal && ifeval.right == EvalValue::String("2".to_string()))
        );

        // Arithmetic and functions stay text, which doesn't compare with a number.
        for expression in ["1 + 1 == 2", "7 / 2 == 3", "max(1, 9) == 9"] {
            assert!(
                matches!(
                    evaluate_ifeval(expression, &DocumentAttributes::default()),
                    Err(Error::InvalidIfEvalDirectiveMismatchedTypes(..))
                ),
                "{expression}"
            );
        }
        // Text operands compare as written, not by their arithmetic value.
        assert!(!evaluate_ifeval(
            "1+1 == 1 + 1",
            &DocumentAttributes::default()
        )?);

        let mut attributes = DocumentAttributes::default();
        assert!(attributes.set("power".into(), "2^10".into()).is_ok());
        assert!(matches!(
            evaluate_ifeval("{power} > 1000", &attributes),
            Err(Error::InvalidIfEvalDirectiveMismatchedTypes(..))
        ));
        assert!(evaluate_ifeval("{power} == 2^10", &attributes)?);
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
    fn quoted_ifeval_rejects_invalid_directives_without_panicking() {
        for line in [
            "ifeval::[\"<tag>\" \"<tag>\"]",
            "ifeval::[\"<tag>\" ==]",
            "ifeval::[\"<tag>\" == \"<tag>\"] trailing",
            "ifeval::[\"<tag> == \"<tag>\"]",
            "ifeval::[' == ']",
        ] {
            assert!(
                matches!(
                    parse_line(line, 1, 0, None),
                    Err(Error::InvalidConditionalDirective(..))
                ),
                "unexpected result for {line:?}"
            );
        }
    }

    #[test]
    fn quoted_ifeval_retains_mismatched_type_errors() -> Result<(), Error> {
        for line in ["ifeval::[\"2\" > 1]", "ifeval::['true' == true]"] {
            let conditional = parse_line(line, 1, 0, None)?;
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
        }
        Ok(())
    }

    fn evaluate_ifeval(expression: &str, attributes: &DocumentAttributes) -> Result<bool, Error> {
        parse_line(&format!("ifeval::[{expression}]"), 1, 0, None)?.is_true(
            attributes,
            &mut String::new(),
            1,
            0,
            None,
        )
    }

    #[rstest::rstest]
    #[case::equal_beyond_f64_precision("9007199254740992 == 9007199254740993", false)]
    #[case::not_equal_beyond_f64_precision("9007199254740992 != 9007199254740993", true)]
    #[case::less_beyond_f64_precision("9007199254740992 < 9007199254740993", true)]
    #[case::negative_beyond_f64_precision("-9007199254740993 < -9007199254740992", true)]
    #[case::near_i64_max("9223372036854775807 == 9223372036854775806", false)]
    #[case::beyond_i64("18446744073709551616 == 18446744073709551617", false)]
    #[case::integer_float_not_rounded("9007199254740993 == 9007199254740992.0", false)]
    #[case::integer_above_float("9007199254740993 > 9007199254740992.0", true)]
    #[case::float_below_integer("9007199254740992.0 < 9007199254740993", true)]
    #[case::integer_equals_float("2 == 2.0", true)]
    #[case::negative_zero("-0.0 == 0", true)]
    #[case::integer_below_fraction("2 < 2.5", true)]
    #[case::negative_integer_above_fraction("-2 > -2.5", true)]
    #[case::fraction_below_integer("1.5 < 2", true)]
    fn ifeval_compares_numbers_exactly(
        #[case] expression: &str,
        #[case] expected: bool,
    ) -> Result<(), Error> {
        assert_eq!(
            evaluate_ifeval(expression, &DocumentAttributes::default())?,
            expected,
            "{expression}"
        );
        Ok(())
    }

    #[test]
    fn ifeval_compares_attribute_integers_exactly() -> Result<(), Error> {
        let mut attributes = DocumentAttributes::default();
        assert!(
            attributes
                .set("big".into(), "9007199254740993".into())
                .is_ok()
        );
        assert!(!evaluate_ifeval("{big} == 9007199254740992", &attributes)?);
        assert!(evaluate_ifeval("{big} > 9007199254740992", &attributes)?);
        Ok(())
    }

    #[test]
    fn integer_float_comparison_handles_i128_limits() {
        let bound = Number::I128_BOUND;
        assert_eq!(
            Number::compare_integer_to_float(i128::MIN, -bound),
            Some(Ordering::Equal)
        );
        assert_eq!(
            Number::compare_integer_to_float(i128::MAX, bound),
            Some(Ordering::Less)
        );
        assert_eq!(
            Number::compare_integer_to_float(i128::MAX, bound.next_down()),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Number::compare_integer_to_float(i128::MIN, (-bound).next_down()),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Number::compare_integer_to_float(i128::MAX, f64::INFINITY),
            Some(Ordering::Less)
        );
        assert_eq!(
            Number::compare_integer_to_float(i128::MIN, f64::NEG_INFINITY),
            Some(Ordering::Greater)
        );
        assert_eq!(Number::compare_integer_to_float(0, f64::NAN), None);
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
