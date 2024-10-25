use std::{collections::HashMap, path::Path};

use crate::error::Error;

use include::Include;

#[derive(Debug, Default)]
pub(crate) struct Preprocessor;

mod include {
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        str::FromStr,
    };

    use pest::Parser as _;
    use pest_derive::Parser;
    use url::Url;

    use crate::error::Error;

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
        lines: Vec<LinesRange>,
        tags: Vec<String>,
        indent: Option<usize>,
        encoding: Option<String>,
        opts: Vec<String>,
    }

    #[derive(Debug)]
    enum LinesRange {
        Single(usize),
        Range(usize, isize),
    }

    #[derive(Debug)]
    pub(crate) enum Target {
        Path(PathBuf),
        Url(Url),
    }

    impl FromStr for LinesRange {
        type Err = Error;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            if s.contains("..") {
                let mut parts = s.split("..");
                let start = parts.next().expect("no start").parse()?;
                let end = parts.next().expect("no end").parse()?;
                Ok(LinesRange::Range(start, end))
            } else {
                Ok(LinesRange::Single(s.parse().map_err(|e| {
                    tracing::error!(?s, "failed to parse line number: {:?}", e);
                    e
                })?))
            }
        }
    }

    impl LinesRange {
        fn parse(value: &str) -> Result<Vec<Self>, Error> {
            let mut lines = Vec::new();
            if value.contains(';') {
                lines.extend(
                    value
                        .split(';')
                        .map(LinesRange::from_str)
                        .collect::<Result<Vec<_>, _>>()?,
                );
            } else if value.contains(',') {
                lines.extend(
                    value
                        .split(',')
                        .map(LinesRange::from_str)
                        .collect::<Result<Vec<_>, _>>()?,
                );
            } else {
                lines.push(LinesRange::from_str(value)?);
            }
            Ok(lines)
        }
    }

    #[derive(Parser, Debug)]
    #[grammar_inline = r#"WHITESPACE = _{ " " | "\t" }
include = _{ SOI ~ "include::" ~ target ~ "[" ~ attributes? ~ "]" }

target = { !WHITESPACE ~ (path_char | url_char)+ }

path_char = _{ ASCII_ALPHANUMERIC | "_" | "/" | "-" | "." | "~" | ":" | "{" | "}" }
url_char = _{ path_char | "?" | "&" | "=" | "%" }

attributes = _{ attribute_pair ~ ("," ~ attribute_pair)* }
attribute_pair = _{ attribute_key ~ "=" ~ attribute_value }

attribute_key = { "leveloffset" | "lines" | "tag" | "tags" | "indent" | "encoding" | "opts" }
attribute_value = {
  ("\"" ~ (!("\"") ~ ANY)+ ~ "\"") |
  (!("," | "]") ~ ANY)+
}"#]
    pub(crate) struct Parser;

    impl Include {
        pub(crate) fn parse(
            file_parent: &Path,
            line: &str,
            attributes: &HashMap<String, String>,
        ) -> Result<Self, Error> {
            let file_parent = file_parent.to_path_buf();
            let mut target = Target::Path(PathBuf::new());
            let mut level_offset = None;
            let mut lines = Vec::new();
            let mut tags = Vec::new();
            let mut indent = None;
            let mut encoding = None;
            let mut opts = Vec::new();

            if let Ok(pairs) = Parser::parse(Rule::include, line) {
                let mut key = "";
                for pair in pairs {
                    match pair.as_rule() {
                        Rule::attribute_key => {
                            key = pair.as_str();
                        }
                        Rule::attribute_value => {
                            let mut value = pair.as_str();
                            if value.starts_with('"') {
                                value = &value[1..value.len() - 1];
                            }
                            match key {
                                "leveloffset" => {
                                    level_offset = Some(value.parse().map_err(|_| {
                                        Error::InvalidLevelOffset(value.to_string())
                                    })?);
                                }
                                "lines" => {
                                    lines.extend(LinesRange::parse(value).map_err(|e| {
                                        tracing::error!(
                                            ?value,
                                            "failed to parse lines attribute: {:?}",
                                            e
                                        );
                                        e
                                    })?);
                                }
                                "tag" => {
                                    tags.push(value.to_string());
                                }
                                "tags" => {
                                    tags.extend(value.split(';').map(str::to_string));
                                }
                                "indent" => {
                                    indent =
                                        Some(value.parse().map_err(|_| {
                                            Error::InvalidIndent(value.to_string())
                                        })?);
                                }
                                "encoding" => {
                                    encoding = Some(value.to_string());
                                }
                                "opts" => {
                                    opts.extend(value.split(',').map(str::to_string));
                                }
                                unknown => {
                                    tracing::error!(
                                        ?unknown,
                                        "unknown attribute key in include directive"
                                    );
                                    return Err(Error::InvalidIncludeDirective);
                                }
                            }
                        }
                        Rule::target => {
                            let target_raw = pair.as_str().trim();
                            let target_raw =
                                super::resolve_attribute_references(attributes, target_raw);
                            target = if target_raw.starts_with("http://")
                                || target_raw.starts_with("https://")
                            {
                                Target::Url(Url::parse(&target_raw)?)
                            } else {
                                Target::Path(PathBuf::from(target_raw))
                            };
                        }
                        unknown => {
                            tracing::warn!(?unknown, "unknown rule in include directive");
                        }
                    }
                }
            } else {
                tracing::error!("failed to parse include directive");
                return Err(Error::InvalidIncludeDirective);
            }
            Ok(Self {
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

        pub(crate) fn lines(&self) -> Result<Vec<String>, Error> {
            // TODO(nlopes): need to read the file according to the properties of the include directive.
            //
            // Right now, this is a simplified version that reads the file as is.
            let mut lines = Vec::new();
            match &self.target {
                Target::Path(path) => {
                    let path = self.file_parent.join(path);
                    let content = super::Preprocessor.process_file(&path).map_err(|e| {
                        tracing::error!(?path, "failed to process file: {:?}", e);
                        e
                    })?;
                    let content_lines = content.lines().map(str::to_string).collect::<Vec<_>>();
                    if let Some(level_offset) = self.level_offset {
                        tracing::warn!(level_offset, "level offset is not supported yet");
                    }
                    if !self.tags.is_empty() {
                        tracing::warn!(tags = ?self.tags, "tags are not supported yet");
                    }
                    if let Some(indent) = self.indent {
                        tracing::warn!(indent, "indent is not supported yet");
                    }
                    if let Some(encoding) = &self.encoding {
                        tracing::warn!(encoding, "encoding is not supported yet");
                    }
                    if !self.opts.is_empty() {
                        tracing::warn!(opts = ?self.opts, "opts are not supported yet");
                    }
                    // TODO(nlopes): this is so unoptimized, it isn't even funny but I'm
                    // trying to just get to a place of compatibility, then I can
                    // optimize.
                    if self.lines.is_empty() {
                        lines.extend(content_lines);
                    } else {
                        for line in &self.lines {
                            match line {
                                LinesRange::Single(line_number) => {
                                    if *line_number < 1 {
                                        // TODO(nlopes): Skip invalid line numbers or should we return an error?
                                        tracing::warn!(
                                            ?line_number,
                                            "invalid line number in include directive"
                                        );
                                        continue;
                                    }
                                    let line_number = line_number - 1;
                                    if line_number < content_lines.len() {
                                        lines.push(content_lines[line_number].clone());
                                    }
                                }
                                LinesRange::Range(start, end) => {
                                    let raw_size = content_lines.len();
                                    if *start < 1 {
                                        // Skip invalid line numbers
                                        tracing::warn!(
                                            ?start,
                                            "invalid start line number in include directive"
                                        );
                                        continue;
                                    }
                                    let start = *start - 1;
                                    let end = if *end == -1 {
                                        raw_size
                                    } else if *end > 0 {
                                        match (*end - 1).try_into() {
                                            Ok(end) => end,
                                            Err(e) => {
                                                tracing::error!(
                                                    ?end,
                                                    "failed to cast end line number to usize: {:?}",
                                                    e
                                                );
                                                continue;
                                            }
                                        }
                                    } else {
                                        // Skip invalid line numbers
                                        tracing::error!(
                                            ?end,
                                            "invalid end line number in include directive"
                                        );
                                        continue;
                                    };
                                    if start < raw_size && end < raw_size {
                                        for line in &content_lines[start..=end] {
                                            lines.push(line.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Target::Url(_url) => {
                    todo!("need to fetch the URL and read its content");
                }
            }
            Ok(lines)
        }
    }
}

mod conditional {
    use std::collections::HashMap;

    use pest::Parser as _;
    use pest_derive::Parser;

    use crate::error::Error;

    #[derive(Debug)]
    pub(crate) enum Conditional {
        Ifdef(Ifdef),
        Ifndef(Ifndef),
        Ifeval(Ifeval),
    }

    #[derive(Debug)]
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
        #[allow(dead_code)]
        expression: String,
    }

    impl Conditional {
        pub(crate) fn is_true(
            &self,
            attributes: &HashMap<String, String>,
            content: &mut String,
        ) -> bool {
            match self {
                Conditional::Ifdef(ifdef) => {
                    let mut is_true = false;
                    if ifdef.attributes.is_empty() {
                        tracing::warn!(
                            "no attributes in ifdef directive but expecting at least one"
                        );
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
                        tracing::warn!(
                            "no attributes in ifndef directive but expecting at least one"
                        );
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

    #[derive(Parser, Debug)]
    #[grammar_inline = r#"WHITESPACE = _{ " " | "\t" }
conditional = _{ ifdef | ifndef | ifeval }

ifdef = { SOI ~ "ifdef::" ~ attributes ~ "[" ~ content? ~ "]" }
ifndef = { SOI ~ "ifndef::" ~ attributes ~ "[" ~ content? ~ "]" }
ifeval = { SOI ~ "ifeval::[" ~ expression ~ "]" }

attributes = _{ name ~ ((or ~ name)+ | (and ~ name)+)? }

name = { (!("[" | or | and) ~ ANY)+ }
or = { "," }
and = { "+" }

content = { (!"]" ~ ANY)+ }
expression = { (!"]" ~ ANY)+ }
"#]
    pub(crate) struct Parser;

    #[tracing::instrument(level = "trace")]
    pub(crate) fn parse_line(
        attributes: &mut std::collections::HashMap<String, String>,
        line: &str,
    ) -> Result<Conditional, Error> {
        match Parser::parse(Rule::conditional, line) {
            Ok(pairs) => {
                let mut conditional = Conditional::Ifdef(Ifdef {
                    attributes: Vec::new(),
                    content: None,
                    operation: None,
                });
                for pair in pairs {
                    match pair.as_rule() {
                        Rule::ifdef => {
                            conditional = parse_ifdef(attributes, pair)?;
                        }
                        Rule::ifndef => {
                            conditional = parse_ifndef(attributes, pair)?;
                        }
                        Rule::ifeval => {
                            conditional = parse_ifeval(attributes, pair)?;
                        }
                        unknown => {
                            tracing::warn!(?unknown, "unknown rule in conditional directive");
                        }
                    }
                }
                Ok(conditional)
            }
            Err(e) => {
                tracing::error!(?e, "failed to parse conditional directive");
                Err(Error::InvalidConditionalDirective)
            }
        }
    }

    #[tracing::instrument(level = "trace")]
    fn parse_ifdef(
        attributes: &mut std::collections::HashMap<String, String>,
        pair: pest::iterators::Pair<Rule>,
    ) -> Result<Conditional, Error> {
        let mut attributes = Vec::new();
        let mut content = None;
        let mut operation = None;

        for pair in pair.into_inner() {
            match pair.as_rule() {
                Rule::name => {
                    attributes.push(pair.as_str().to_string());
                }
                Rule::and => {
                    operation = Some(Operation::And);
                }
                Rule::or => {
                    operation = Some(Operation::Or);
                }
                Rule::content => {
                    content = Some(pair.as_str().to_string());
                }
                unknown => {
                    tracing::warn!(?unknown, "unknown rule in ifdef directive");
                }
            }
        }

        Ok(Conditional::Ifdef(Ifdef {
            attributes,
            content,
            operation,
        }))
    }

    #[tracing::instrument(level = "trace")]
    fn parse_ifndef(
        attributes: &mut std::collections::HashMap<String, String>,
        pair: pest::iterators::Pair<Rule>,
    ) -> Result<Conditional, Error> {
        let mut attributes = Vec::new();
        let mut content = None;
        let mut operation = None;

        for pair in pair.into_inner() {
            match pair.as_rule() {
                Rule::name => {
                    attributes.push(pair.as_str().to_string());
                }
                Rule::and => {
                    operation = Some(Operation::And);
                }
                Rule::or => {
                    operation = Some(Operation::Or);
                }
                Rule::content => {
                    content = Some(pair.as_str().to_string());
                }
                unknown => {
                    tracing::warn!(?unknown, "unknown rule in ifndef directive");
                }
            }
        }

        Ok(Conditional::Ifndef(Ifndef {
            attributes,
            content,
            operation,
        }))
    }

    #[tracing::instrument(level = "trace")]
    fn parse_ifeval(
        attributes: &mut std::collections::HashMap<String, String>,
        pair: pest::iterators::Pair<Rule>,
    ) -> Result<Conditional, Error> {
        let mut expression = String::new();

        for pair in pair.into_inner() {
            match pair.as_rule() {
                Rule::expression => {
                    expression = pair.as_str().to_string();
                }
                unknown => {
                    tracing::warn!(?unknown, "unknown rule in ifeval directive");
                }
            }
        }

        Ok(Conditional::Ifeval(Ifeval { expression }))
    }
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
    fn normalize(input: &str) -> String {
        input
            .lines()
            .map(str::trim_end)
            .collect::<Vec<&str>>()
            .join("\n")
    }

    #[tracing::instrument]
    pub fn process(&self, input: &str) -> Result<String, Error> {
        self.process_either(input, None)
    }

    #[tracing::instrument(skip(file_path))]
    pub fn process_file<P: AsRef<Path>>(&self, file_path: P) -> Result<String, Error> {
        let file_parent = file_path
            .as_ref()
            .parent()
            .expect("file path has no parent");

        let input = std::fs::read_to_string(&file_path).map_err(|e| {
            tracing::error!(
                path = ?file_path.as_ref().display(),
                "failed to read file: {:?}",
                e
            );
            e
        })?;
        self.process_either(&input, Some(file_parent))
    }

    #[tracing::instrument]
    fn process_either(&self, input: &str, file_parent: Option<&Path>) -> Result<String, Error> {
        let input = Preprocessor::normalize(input);
        let mut attributes = HashMap::new();

        let mut output = Vec::new();
        let mut lines = input.lines().peekable();
        while let Some(line) = lines.next() {
            if line.starts_with(':') {
                attribute::parse_line(&mut attributes, line);
            }
            if line.starts_with("----") {
                let mut keep_lines = vec![line.to_string()];
                let mut terminated = false;
                // Skip the block
                while let Some(next_line) = lines.peek() {
                    if next_line.starts_with("----") {
                        terminated = true;
                        lines.next();
                        break;
                    } else {
                        keep_lines.push(next_line.to_string());
                    }
                    lines.next();
                }
                if terminated {
                    output.extend(keep_lines);
                }
            }
            // Taken from
            // https://github.com/asciidoctor/asciidoctor/blob/306111f480e2853ba59107336408de15253ca165/lib/asciidoctor/reader.rb#L604
            // while following the specs at
            // https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/outline.adoc?ref_type=heads#user-content-preprocessor
            if line.ends_with(']') && !line.starts_with('[') && line.contains("::") {
                if line.starts_with("\\include")
                    || line.starts_with("\\ifdef")
                    || line.starts_with("\\ifndef")
                    || line.starts_with("\\ifeval")
                {
                    // Return the directive as is
                    output.push(line[1..].to_string());
                } else if line.starts_with("ifdef")
                    || line.starts_with("ifndef")
                    || line.starts_with("ifeval")
                {
                    let mut content = String::new();
                    let condition = conditional::parse_line(&mut attributes, line)?;
                    while let Some(next_line) = lines.peek() {
                        if next_line.is_empty() {
                            tracing::trace!(?line, "single line if directive");
                            break;
                        } else if next_line.starts_with("endif") {
                            tracing::trace!(?content, "multiline if directive");
                            // Skip the if/endif block
                            lines.next();
                            break;
                        }
                        content.push_str(&format!("{next_line}\n"));
                        lines.next();
                    }
                    if condition.is_true(&attributes, &mut content) {
                        output.push(content);
                    }
                } else if line.starts_with("include") {
                    if let Some(file_parent) = file_parent {
                        // Parse the include directive
                        let include = Include::parse(file_parent, line, &attributes)?;
                        // Process the include directive
                        output.extend(include.lines()?);
                    } else {
                        tracing::error!(
                            "file parent is missing - include directive cannot be processed"
                        );
                    }
                } else {
                    // Return the directive as is
                    output.push(line.to_string());
                }
            } else {
                // Return the line as is
                output.push(line.to_string());
            }
        }

        Ok(format!("{}\n", output.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_attribute_references() {
        let mut attributes = HashMap::new();
        attributes.insert("name".to_string(), "value".to_string());
        attributes.insert("name2".to_string(), "value2".to_string());

        let value = "{name}";
        let resolved = resolve_attribute_references(&attributes, value);
        assert_eq!(resolved, "value");

        let value = "{name} {name2}";
        let resolved = resolve_attribute_references(&attributes, value);
        assert_eq!(resolved, "value value2");

        let value = "{name} {name3}";
        let resolved = resolve_attribute_references(&attributes, value);
        assert_eq!(resolved, "value {name3}");

        let value = "{name3}";
        let resolved = resolve_attribute_references(&attributes, value);
        assert_eq!(resolved, "{name3}");
    }

    #[test]
    fn test_process() {
        let preprocessor = Preprocessor::default();
        let input = r#":attribute: value

ifdef::attribute[]
content
endif::[]
"#;
        let output = preprocessor.process(input).unwrap();
        assert_eq!(
            output,
            r#":attribute: value

content

"#
        );
    }
}
