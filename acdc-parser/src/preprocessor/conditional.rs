use crate::{
    DocumentAttributes,
    error::Error,
    model::{HEADER, Substitute},
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
            = op:$("==" / "!=" / "<=" / ">=" / "<" / ">") {
                match op {
                    "==" => Operator::Equal,
                    "!=" => Operator::NotEqual,
                    "<" => Operator::LessThan,
                    ">" => Operator::GreaterThan,
                    "<=" => Operator::LessThanOrEqual,
                    ">=" => Operator::GreaterThanOrEqual,
                    _ => unreachable!(),
                }
            }

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
    pub(crate) fn is_true(
        &self,
        attributes: &DocumentAttributes,
        content: &mut String,
    ) -> Result<bool, Error> {
        Ok(match self {
            Conditional::Ifdef(ifdef) => {
                let mut is_true = false;
                if ifdef.attributes.is_empty() {
                    tracing::warn!("no attributes in ifdef directive but expecting at least one");
                } else if let Some(Operation::Or) = &ifdef.operation {
                    is_true = ifdef
                        .attributes
                        .iter()
                        .any(|attr| attributes.contains_key(attr));
                } else {
                    // Operation::And (or just one attribute)
                    is_true = ifdef
                        .attributes
                        .iter()
                        .all(|attr| attributes.contains_key(attr));
                }
                if is_true {
                    if let Some(if_content) = &ifdef.content {
                        content.clone_from(if_content);
                    }
                }
                is_true
            }
            Conditional::Ifndef(ifndef) => {
                let mut is_true = true;
                if ifndef.attributes.is_empty() {
                    tracing::warn!("no attributes in ifndef directive but expecting at least one");
                } else if let Some(Operation::Or) = &ifndef.operation {
                    is_true = !ifndef
                        .attributes
                        .iter()
                        .any(|attr| attributes.contains_key(attr));
                } else {
                    // Operation::And (or just one attribute)
                    is_true = !ifndef
                        .attributes
                        .iter()
                        .all(|attr| attributes.contains_key(attr));
                }
                if is_true {
                    if let Some(if_content) = &ifndef.content {
                        content.clone_from(if_content);
                    }
                }
                is_true
            }
            Conditional::Ifeval(ifeval) => ifeval.evaluate(attributes)?,
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
    #[tracing::instrument(level = "trace")]
    fn evaluate(&self, attributes: &DocumentAttributes) -> Result<bool, Error> {
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
                return Err(Error::InvalidIfEvalDirectiveMismatchedTypes);
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

                // Now, we try to parse the string into a number or a boolean if
                // possible. If not, we assume it's a string and return it as is.
                if let Ok(value) = s.parse::<bool>() {
                    EvalValue::Boolean(value)
                } else if let Ok(value) = s.parse::<f64>() {
                    EvalValue::Number(value)
                } else {
                    // If we're here, let's check if we can evaluate this as a math expression
                    // and return the result as a number.
                    //
                    // If not, we return the string as is.
                    if let Ok(value) = evalexpr::eval_float(&s) {
                        EvalValue::Number(value)
                    } else if let Ok(value) = evalexpr::eval_int(&s) {
                        // We have to have this here because evalexpr::eval_float may
                        // return an error if the parsed number is an integer.
                        //
                        // That means that if we don't get a flot, we try to parse an int.
                        //
                        // Because we store everything as a float, we have to convert the
                        // int to a float.
                        if let Ok(value) = format!("{value}").parse::<f64>() {
                            EvalValue::Number(value)
                        } else {
                            tracing::error!(value, "failed to parse i64 as f64");
                            EvalValue::String(Self::strip_quotes(s))
                        }
                    } else {
                        EvalValue::String(Self::strip_quotes(s))
                    }
                }
            }
            value => value.clone(),
        }
    }

    #[tracing::instrument(level = "trace")]
    fn strip_quotes(s: String) -> String {
        if s.starts_with('\'') && s.ends_with('\'') {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }
}

#[tracing::instrument(level = "trace")]
pub(crate) fn parse_line(line: &str) -> Result<Conditional, Error> {
    match conditional_parser::conditional(line) {
        Ok(conditional) => Ok(conditional),
        Err(e) => {
            tracing::error!(error=?e, "failed to parse conditional directive");
            Err(Error::InvalidConditionalDirective)
        }
    }
}

#[tracing::instrument(level = "trace")]
pub(crate) fn parse_endif(line: &str) -> Result<Endif, Error> {
    match conditional_parser::endif(line) {
        Ok(endif) => Ok(endif),
        Err(e) => {
            tracing::error!(error=?e, "failed to parse endif directive");
            Err(Error::InvalidConditionalDirective)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ifdef_single_attribute() {
        let line = "ifdef::attribute[]";
        let conditional = parse_line(line).unwrap();
        match conditional {
            Conditional::Ifdef(ifdef) => {
                assert_eq!(ifdef.attributes, vec!["attribute"]);
                assert_eq!(ifdef.operation, None);
                assert_eq!(ifdef.content, None);
            }
            _ => panic!("Expected Ifdef"),
        }
    }

    #[test]
    fn test_ifdef_or_attributes() {
        let line = "ifdef::attr1,attr2[]";
        let conditional = parse_line(line).unwrap();
        match conditional {
            Conditional::Ifdef(ifdef) => {
                assert_eq!(ifdef.attributes, vec!["attr1", "attr2"]);
                assert_eq!(ifdef.operation, Some(Operation::Or));
                assert_eq!(ifdef.content, None);
            }
            _ => panic!("Expected Ifdef"),
        }
    }

    #[test]
    fn test_ifdef_and_attributes() {
        let line = "ifdef::attr1+attr2[]";
        let conditional = parse_line(line).unwrap();
        match conditional {
            Conditional::Ifdef(ifdef) => {
                assert_eq!(ifdef.attributes, vec!["attr1", "attr2"]);
                assert_eq!(ifdef.operation, Some(Operation::And));
                assert_eq!(ifdef.content, None);
            }
            _ => panic!("Expected Ifdef"),
        }
    }

    #[test]
    fn test_ifndef() {
        let line = "ifndef::attribute[]";
        let conditional = parse_line(line).unwrap();
        match conditional {
            Conditional::Ifndef(ifndef) => {
                assert_eq!(ifndef.attributes, vec!["attribute"]);
                assert_eq!(ifndef.operation, None);
                assert_eq!(ifndef.content, None);
            }
            _ => panic!("Expected Ifndef"),
        }
    }

    #[test]
    fn test_ifeval_simple_math() {
        let line = "ifeval::[1 + 1 == 2]";
        let conditional = parse_line(line).unwrap();
        match &conditional {
            Conditional::Ifeval(ifeval) => {
                assert_eq!(ifeval.left, EvalValue::String("1 + 1".to_string()));
                assert_eq!(ifeval.operator, Operator::Equal);
                assert_eq!(ifeval.right, EvalValue::String("2".to_string()));
            }
            _ => panic!("Expected Ifeval"),
        }
        assert!(
            conditional
                .is_true(&DocumentAttributes::default(), &mut String::new())
                .unwrap()
        );
    }

    #[test]
    fn test_ifeval_str_equality() {
        let line = "ifeval::['ASDF' == ASDF]";
        let conditional = parse_line(line).unwrap();
        match &conditional {
            Conditional::Ifeval(ifeval) => {
                assert_eq!(ifeval.left, EvalValue::String("'ASDF'".to_string()));
                assert_eq!(ifeval.operator, Operator::Equal);
                assert_eq!(ifeval.right, EvalValue::String("ASDF".to_string()));
            }
            _ => panic!("Expected Ifeval"),
        }
        assert!(
            conditional
                .is_true(&DocumentAttributes::default(), &mut String::new())
                .unwrap()
        );
    }

    #[test]
    fn test_ifeval_greater_than_string_vs_number() {
        let line = "ifeval::['1+1' >= 2]";
        let conditional = parse_line(line).unwrap();
        match &conditional {
            Conditional::Ifeval(ifeval) => {
                assert_eq!(ifeval.left, EvalValue::String("'1+1'".to_string()));
                assert_eq!(ifeval.operator, Operator::GreaterThanOrEqual);
                assert_eq!(ifeval.right, EvalValue::String("2".to_string()));
                assert!(matches!(
                    ifeval.evaluate(&DocumentAttributes::default()),
                    Err(Error::InvalidIfEvalDirectiveMismatchedTypes)
                ));
            }
            _ => panic!("Expected Ifeval"),
        }
    }

    #[test]
    fn test_ifdef_with_content() {
        let line = "ifdef::attribute[Some content here]";
        let conditional = parse_line(line).unwrap();
        match conditional {
            Conditional::Ifdef(ifdef) => {
                assert_eq!(ifdef.attributes, vec!["attribute"]);
                assert_eq!(ifdef.operation, None);
                assert_eq!(ifdef.content, Some("Some content here".to_string()));
            }
            _ => panic!("Expected Ifdef"),
        }
    }

    #[test]
    fn test_endif() {
        let line = "endif::attribute[]";
        let endif = parse_endif(line).unwrap();
        assert_eq!(endif.attribute, Some("attribute".to_string()));
    }

    #[test]
    fn test_endif_no_attribute() {
        let line = "endif::[]";
        let endif = parse_endif(line).unwrap();
        assert_eq!(endif.attribute, None);
    }
}
