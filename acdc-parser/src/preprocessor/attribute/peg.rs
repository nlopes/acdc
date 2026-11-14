use crate::document_attribute::{AttributeDeclaration, RawAttributeValue};

peg::parser! {
    pub(super) grammar attribute_parser() for str {
        // As in asciidoctor, a value must be separated from `:name:` by
        // whitespace, so `:name:text` is not an attribute entry. An unset
        // entry (`:!name:` or `:name!:`) ignores its value.
        pub(in crate::preprocessor::attribute) rule document_attribute() -> AttributeDeclaration<'input>
            = ":" entry:(
                "!" name:name() { (name, false) }
                / name:name() "!" { (name, false) }
                / name:name() { (name, true) }
            ) ":" value:(whitespace() value:value() { value })? {
                let (name, set) = entry;
                let value = if set {
                    value.map_or(RawAttributeValue::Set, |text| RawAttributeValue::Text(text.into()))
                } else {
                    RawAttributeValue::Unset
                };
                // The document grammar reports ignored unset text when it
                // applies the entry, so the preprocessor doesn't keep it.
                AttributeDeclaration { name, value, ignored_value: None }
            }

        rule name() -> &'input str
            = n:$((['a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_']+)) { n }

        rule value() -> &'input str
            = v:$([^'\n']*) { v }

        rule whitespace() = quiet!{[' ' | '\t']+}
    }
}
