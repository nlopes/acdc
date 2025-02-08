use crate::{error::Error, DocumentAttributes};

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
    expression: String,
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
            = "ifeval::[" expression:content() "]" {
                Conditional::Ifeval(Ifeval {
                    expression,
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
    pub(crate) fn is_true(&self, attributes: &DocumentAttributes, content: &mut String) -> bool {
        match self {
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
            Conditional::Ifeval(_ifeval) => todo!("ifeval conditional check"),
        }
    }
}

impl Endif {
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
    fn test_ifeval() {
        let line = "ifeval::[1 + 1 == 2]";
        let conditional = parse_line(line).unwrap();
        match conditional {
            Conditional::Ifeval(ifeval) => {
                assert_eq!(ifeval.expression, "1 + 1 == 2");
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
