use crate::document_attribute::{AttributeDeclaration, RawAttributeValue};

peg::parser! {
    pub(super) grammar attribute_parser() for str {
        pub(crate) rule document_attribute() -> AttributeDeclaration<'input>
            = ":" "!" name:name() ":" { AttributeDeclaration { name, value: RawAttributeValue::Unset } }
            / ":" name:name() "!" ":" { AttributeDeclaration { name, value: RawAttributeValue::Unset } }
            / ":" name:name() ":" whitespace()? value:value()? {
                AttributeDeclaration {
                    name,
                    value: value.map_or(RawAttributeValue::Set, |text| RawAttributeValue::Text(text.into())),
                }
            }

        rule name() -> &'input str
            = n:$((['a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_']+)) { n }

        rule value() -> &'input str
            = v:$([^'\n']*) { v }

        rule whitespace() = quiet!{[' ' | '\t']+}
    }
}
