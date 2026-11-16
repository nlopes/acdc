use crate::{
    CompatibilityMode,
    document_attribute::{AttributeDeclaration, RawAttributeValue, starts_like_entry_name},
};

peg::parser! {
    pub(super) grammar attribute_parser() for str {
        // As in asciidoctor, a value must be separated from `:name:` by
        // whitespace, so `:name:text` is not an attribute entry. An unset
        // entry (`:!name:` or `:name!:`) ignores its value. Names follow the
        // document grammar, so both read the same entries.
        pub(in crate::preprocessor::attribute) rule document_attribute(mode: CompatibilityMode) -> AttributeDeclaration<'input>
            = ":" entry:(
                entry:(
                    "!" name:$([^':' | '\n']+) { (name, false) }
                    / name:$([^('!' | ':' | '\n')]+) "!" { (name, false) }
                    / name:$([^':' | '\n']+) { (name, true) }
                ) {?
                    if mode == CompatibilityMode::Asciidoctor && !starts_like_entry_name(entry.0) {
                        Err("attribute name starting with a word character")
                    } else {
                        Ok(entry)
                    }
                }
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

        rule value() -> &'input str
            = v:$([^'\n']*) { v }

        rule whitespace() = quiet!{[' ' | '\t']+}
    }
}
