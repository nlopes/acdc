use super::{ContentSelection, Include, IncludeParserInputs, Target};
use crate::{
    error::Error,
    model::{HEADER, apply_attribute_substitutions},
};
use std::{path::Path, rc::Rc};

peg::parser! {
    pub(super) grammar include_parser<'a, 'b>(inputs: &'b IncludeParserInputs<'a, 'b>) for str {
        pub(in crate::preprocessor::include) rule include() -> Result<Include<'a>, Error>
            = "include::" target:target() "[" attrs:attributes()? "]" {
                let target_raw = apply_attribute_substitutions(&target, HEADER, &inputs.options.document_attributes);
                let target_as_written = target_raw.into_owned();
                let target = Target::parse(&target_as_written, inputs.source_origin)?;

                let mut include = Include {
                    source_origin: inputs.source_origin.clone(),
                    target,
                    target_as_written,
                    level_offset: None,
                    selection: ContentSelection::All,
                    indent: None,
                    encoding: None,
                    opts: Vec::new(),
                    options: inputs.options.clone(),
                    context: inputs.context.clone(),
                    line_number: inputs.location.line_number,
                    current_offset: inputs.location.current_offset,
                    current_file: inputs.location.current_file.map(Path::to_path_buf),
                    warnings: Rc::clone(inputs.warnings),
                };
                if let Some(attrs) = attrs {
                    include.parse_attributes(attrs)?;
                }
                Ok(include)
            }

        rule target() -> String
            = t:$((!['['] [_])+)
            {?
                if t == t.trim_ascii() {
                    Ok(t.to_string())
                } else {
                    Err("include target without leading or trailing whitespace")
                }
            }

        rule attributes() -> Vec<(String, String)>
            = pair:attribute_pair() pairs:("," p:attribute_pair() { p })* {
                let mut attrs = vec![pair];
                attrs.extend(pairs);
                attrs
            }

        rule attribute_pair() -> (String, String)
            = k:attribute_key() "=" v:attribute_value() {
                (k, v)
            }

        rule attribute_key() -> String
            = k:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+) {
                k.to_string()
            }

        rule attribute_value() -> String
            = "\"" v:$((!['"'] [_])*) "\"" { v.to_string() }
        / v:$((![','] ![']'] [_])*) { v.to_string() }
    }
}
