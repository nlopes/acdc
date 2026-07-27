use super::{
    ContentSelection, Include, IncludeParserInputs, Target,
    attributes::{AttributeMissingPolicy, substitute_include_attributes},
};
use crate::error::Error;
use std::{path::Path, rc::Rc};

peg::parser! {
    pub(super) grammar include_parser<'a, 'b>(inputs: &'b IncludeParserInputs<'a, 'b>) for str {
        pub(in crate::preprocessor::include) rule include() -> Result<Include<'a>, Error>
            = "include::" target:target() "[" attrs:$((!("]" ![_]) [_])*) "]" {
                let target_missing_policy = AttributeMissingPolicy::from_attributes(&inputs.options.document_attributes);
                let policy = if target_missing_policy == AttributeMissingPolicy::Warn { AttributeMissingPolicy::DropLine } else { target_missing_policy };
                let substitution = substitute_include_attributes(&target, &inputs.options.document_attributes, policy);
                let target_as_written = substitution.value;
                let parsed_target = if target_as_written.is_empty() { Target::Blank } else { Target::parse(&target_as_written, inputs.source_origin)? };

                let mut include = Include {
                    source_origin: inputs.source_origin.clone(),
                    target: parsed_target,
                    target_original: target,
                    target_missing: substitution.missing,
                    target_missing_policy,
                    target_as_written,
                    level_offset: None,
                    selection: ContentSelection::All,
                    indent: None,
                    encoding: None,
                    opts: Vec::new(),
                    depth: None,
                    options: inputs.options.clone(),
                    context: inputs.context.clone(),
                    line_number: inputs.location.line_number,
                    current_file: inputs.location.current_file.map(Path::to_path_buf),
                    warnings: Rc::clone(inputs.warnings),
                };
                include.parse_attributes(attrs)?;
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

    }
}
