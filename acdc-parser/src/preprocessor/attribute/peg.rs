use crate::document_attribute::{AttributeDeclaration, RawAttributeValue};

peg::parser! {
    pub(super) grammar attribute_parser() for str {
        pub(in crate::preprocessor::attribute) rule document_attribute() -> AttributeDeclaration<'input>
            = ":" "!" name:name() ":" { AttributeDeclaration { name, value: RawAttributeValue::Unset } }
            / ":" name:name() "!" ":" { AttributeDeclaration { name, value: RawAttributeValue::Unset } }
            // As in asciidoctor, a value must be separated from `:name:` by
            // whitespace, so `:name:text` is not an attribute entry.
            / ":" name:name() ":" value:(whitespace() value:value() { value })? {
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
