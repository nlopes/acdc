use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use url::Url;

use crate::error::Error;

#[derive(Debug)]
pub(crate) enum Directive {
    Include(Box<Include>),
    Conditional(Conditional),
}

#[derive(Debug, Default)]
pub(crate) struct Preprocessor {
    include_stack: Vec<Include>,
    conditional_stack: Vec<Conditional>,
}

// TODO(nlopes): use pest inline grammar to parse the input. (Like the attribute module)
//
// The format of an include directive is the following:
//
// include::target[leveloffset=offset,lines=ranges,tag(s)=name(s),indent=depth,encoding=encoding,opts=optional]
//
// The target is required. The target may be an absolute path, a path relative to the
// current document, or a URL.
//
// The include directive can be escaped.
//
// If you don’t want the include directive to be processed, you must escape it using a
// backslash.
//
// \include::just-an-example.ext[]
//
// Escaping the directive is necessary even if it appears in a verbatim block since it’s
// not aware of the surrounding document structure.

#[derive(Debug)]
pub(crate) struct Include {
    file_parent: PathBuf,
    target: Target,
    level_offset: Option<isize>,
    lines: Vec<String>,
    tags: Vec<String>,
    indent: Option<usize>,
    encoding: Option<String>,
    opts: Vec<String>,
}

impl Include {
    fn parse(
        file_parent: &Path,
        input: &str,
        attributes: &HashMap<String, String>,
    ) -> Result<Include, Error> {
        let file_parent = file_parent.to_path_buf();
        let mut level_offset = None;
        let mut lines = Vec::new();
        let mut tags = Vec::new();
        let mut indent = None;
        let mut encoding = None;
        let mut opts = Vec::new();

        let mut parts = input.split("::");
        let _ = parts.next();
        let parts = parts.next().expect("no parts");

        let mut parts = parts.split('[');
        let target = parts.next().expect("no target");
        let target = target.trim();
        let target = resolve_attribute_references(attributes, target);
        let target = if target.starts_with("http://") || target.starts_with("https://") {
            Target::Url(Url::parse(&target)?)
        } else {
            Target::Path(PathBuf::from(target))
        };

        let options = parts.next().expect("no opts");
        let options = options.trim().trim_end_matches(']');
        let options = options.split(',');
        for opt in options {
            if opt.is_empty() {
                continue;
            }
            let mut parts = opt.split('=');
            let key = parts.next().expect("no key");
            let value = parts.next().expect("no value");
            match key {
                "leveloffset" => {
                    level_offset = Some(
                        value
                            .parse()
                            .expect("invalid level offset, cannot parse as integer"),
                    );
                }
                "lines" => {
                    todo!("need to parse ranges, a list of them");
                }
                "tag" => {
                    tags.push(value.to_string());
                }
                "indent" => {
                    indent = Some(
                        value
                            .parse()
                            .expect("invalid indent, cannot parse as unsigned integer"),
                    );
                }
                "encoding" => {
                    encoding = Some(value.to_string());
                }
                "opts" => {
                    todo!("need to parse optional arguments");
                }
                _ => {
                    return Err(Error::InvalidIncludeDirective);
                }
            }
        }

        Ok(Include {
            file_parent,
            target,
            level_offset,
            lines,
            tags,
            indent,
            encoding,
            opts,
        })
    }

    fn lines(&self) -> Vec<String> {
        // TODO(nlopes): need to read the file named by the target and living in the file parent directory according to the provided properties.
        let mut lines = Vec::new();
        match &self.target {
            Target::Path(path) => {
                let path = self.file_parent.join(path);
                let content = std::fs::read_to_string(&path).expect("could not read file");
                lines.extend(content.lines().map(str::to_string));
            }
            Target::Url(_url) => {
                todo!("need to fetch the URL and read its content");
            }
        }
        lines
    }
}

#[derive(Debug)]
pub(crate) enum Conditional {
    Ifdef,
    Ifndef,
    Ifeval,
}

#[derive(Debug)]
pub(crate) enum Target {
    Path(PathBuf),
    Url(Url),
}

mod attribute {
    use pest::Parser as _;
    use pest_derive::Parser;
    #[derive(Parser, Debug)]
    #[grammar_inline = r#"WHITESPACE = _{ " " | "\t" }
document_attribute = _{
  SOI ~
  (":" ~ ((unset ~ name) | (name ~ unset)) ~ ":") |
  (":" ~ name ~ ":" ~ value?)
}

unset = { "!" }
name = { (ASCII_ALPHANUMERIC | "-" | "_")+ }
value = { (!EOI ~ ANY)+ }"#]
    pub(crate) struct Parser;

    pub(crate) fn parse_line(
        attributes: &mut std::collections::HashMap<String, String>,
        line: &str,
    ) {
        if let Ok(pairs) = Parser::parse(Rule::document_attribute, line) {
            let mut unset = false;
            let mut name = "";
            let mut value = "";

            for pair in pairs {
                match pair.as_rule() {
                    Rule::name => {
                        name = pair.as_str();
                    }
                    Rule::unset => {
                        unset = true;
                    }
                    Rule::value => {
                        value = pair.as_str();
                    }
                    unknown => {
                        tracing::warn!("unknown rule: {:?}", unknown);
                    }
                }
            }
            if unset {
                attributes.remove(name);
            } else {
                let value = if value.contains('{') && value.contains('}') {
                    super::resolve_attribute_references(attributes, value)
                } else {
                    value.to_string()
                };
                attributes.insert(name.to_string(), value);
            }
        }
    }
}

// Given a text and a set of attributes, resolve the attribute references in the text.
//
// The attribute references are in the form of {name}.
pub fn resolve_attribute_references(attributes: &HashMap<String, String>, value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut i: usize = 0;

    while i < value.len() {
        if value[i..].starts_with('{') {
            if let Some(end_brace) = value[i + 1..].find('}') {
                let attr_name = &value[i + 1..i + 1 + end_brace];
                if let Some(attr_value) = attributes.get(attr_name) {
                    result.push_str(attr_value);
                } else {
                    // TODO(nlopes): this behaves differently depending on the
                    // `attribute-missing` and `attribute-undefined` options.
                    //
                    // Details can be found at:
                    // https://docs.asciidoctor.org/asciidoc/latest/attributes/unresolved-references/
                    result.push('{');
                    result.push_str(attr_name);
                    result.push('}');
                }
                i += end_brace + 2;
            } else {
                result.push_str(&value[i..=i]);
                i += 1;
            }
        } else {
            result.push_str(&value[i..=i]);
            i += 1;
        }
    }

    result
}

impl Preprocessor {
    pub fn new() -> Preprocessor {
        Preprocessor::default()
    }

    fn normalize(input: &str) -> String {
        input
            .lines()
            .map(str::trim_end)
            .collect::<Vec<&str>>()
            .join("\n")
    }

    #[tracing::instrument]
    pub fn process(&self, input: &str) -> String {
        let mut input = Preprocessor::normalize(input);
        input.push('\n');
        input
    }

    #[tracing::instrument(skip(file_path))]
    pub fn process_file<P: AsRef<Path>>(&self, file_path: P) -> Result<String, Error> {
        let file_parent = file_path
            .as_ref()
            .parent()
            .expect("file path has no parent");

        let input = std::fs::read_to_string(&file_path)?;
        let input = Preprocessor::normalize(&input);
        let mut attributes = HashMap::new();

        let mut output = Vec::new();
        for line in input.lines() {
            if line.starts_with(':') {
                attribute::parse_line(&mut attributes, line);
            }
            // Taken from https://github.com/asciidoctor/asciidoctor/blob/306111f480e2853ba59107336408de15253ca165/lib/asciidoctor/reader.rb#L604
            if line.ends_with(']') && !line.starts_with('[') && line.contains("::") {
                if line.starts_with("\\include")
                    || line.starts_with("\\ifdef")
                    || line.starts_with("\\ifndef")
                    || line.starts_with("\\ifeval")
                {
                    // Return the directive as is
                    output.push(line[1..].to_string());
                } else if line.starts_with("ifdef") {
                } else if line.starts_with("include") {
                    // Parse the include directive
                    let include = Include::parse(file_parent, line, &attributes)?;
                    // Process the include directive
                    output.extend(include.lines());
                } else {
                    // Return the directive as is
                    output.push(line.to_string());
                }
            } else {
                // Return the line as is
                output.push(line.to_string());
            }
        }
        dbg!(&attributes);

        Ok(format!("{}\n", output.join("\n")))
    }
}
