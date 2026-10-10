use super::{
    AttributeCondition, Condition, Conditional, Endif, EvalCondition, EvalValue, Operation,
    Operator,
};

peg::parser! {
    pub(super) grammar conditional_parser() for str {
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

                // Keep operands as text until attribute substitution determines their types.
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
            // A complete quoted operand can contain operators and brackets.
            = n:$( [' ' | '\t']* (
                "\"" (!"\"" [_])* "\""
                / "'" (!"'" [_])* "'"
            ) [' ' | '\t']*) &(operator() / "]") {
                n.trim().to_string()
            }
            / n:$((!operator() ![']'] [_])+)  {
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
