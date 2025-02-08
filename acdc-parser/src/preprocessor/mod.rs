//! The preprocessor module is responsible for processing the input document and expanding include directives.
use std::path::Path;

use crate::{error::Error, DocumentAttributes};

mod attribute;
mod conditional;
mod include;

use include::Include;

#[derive(Debug, Default)]
pub(crate) struct Preprocessor;

impl Preprocessor {
    fn normalize(input: &str) -> String {
        input
            .lines()
            .map(str::trim_end)
            .collect::<Vec<&str>>()
            .join("\n")
    }

    #[tracing::instrument(skip(reader))]
    pub fn process_reader<R: std::io::Read>(&self, mut reader: R) -> Result<String, Error> {
        let mut input = String::new();
        reader.read_to_string(&mut input).map_err(|e| {
            tracing::error!("failed to read from reader: {:?}", e);
            e
        })?;
        self.process(&input)
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
        let mut attributes = DocumentAttributes::default();
        let mut output = Vec::new();
        let mut lines = input.lines().peekable();
        while let Some(line) = lines.next() {
            if line.starts_with(':') && (line.ends_with(" + \\") || line.ends_with(" \\")) {
                let mut attribute_content = String::new();
                if line.ends_with(" + \\") {
                    attribute_content.push_str(line);
                    attribute_content.push('\n');
                } else if line.ends_with(" \\") {
                    attribute_content.push_str(line.trim_end_matches('\\'));
                }
                while let Some(next_line) = lines.peek() {
                    let next_line = next_line.trim();
                    // If the next line isn't the end of a continuation, or a
                    // continuation, we need to break out.
                    if next_line.starts_with(':') || next_line.is_empty() {
                        break;
                    }
                    // If we get here, and we get a hard wrap, keep everything as is.
                    // If we get here, and we get a soft wrap, then remove the newline.
                    // Anything else means we're at the end of the wrapped attribute, so
                    // feed it and break.
                    if next_line.ends_with(" + \\") {
                        attribute_content.push_str(next_line);
                        attribute_content.push('\n');
                        lines.next();
                    } else if next_line.ends_with(" \\") {
                        attribute_content.push_str(next_line.trim_end_matches('\\'));
                        lines.next();
                    } else {
                        attribute_content.push_str(next_line);
                        lines.next();
                        break;
                    }
                }
                attribute::parse_line(&mut attributes, attribute_content.as_str());
                output.push(attribute_content);
                continue;
            } else if line.starts_with(':') {
                attribute::parse_line(&mut attributes, line.trim());
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
                    let condition = conditional::parse_line(line)?;
                    while let Some(next_line) = lines.peek() {
                        if next_line.is_empty() {
                            tracing::trace!(?line, "single line if directive");
                            break;
                        } else if next_line.starts_with("endif") {
                            let endif = conditional::parse_endif(next_line)?;
                            if !endif.closes(&condition) {
                                tracing::warn!(
                                    "attribute mismatch between if and endif directives"
                                );
                                return Err(Error::InvalidConditionalDirective);
                            }
                            tracing::trace!(?content, "multiline if directive");
                            // Skip the if/endif block
                            lines.next();
                            break;
                        }
                        content.push_str(&format!("{next_line}\n"));
                        lines.next();
                    }
                    if condition.is_true(&attributes, &mut content)? {
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

        Ok(output.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process() {
        let input = ":attribute: value

ifdef::attribute[]
content
endif::[]
";
        let output = Preprocessor.process(input).unwrap();
        assert_eq!(output, ":attribute: value\n\ncontent\n");
    }

    #[test]
    fn test_good_endif_directive() {
        let input = ":asdf:

ifdef::asdf[]
content
endif::asdf[]";
        let output = Preprocessor.process(input).unwrap();
        assert_eq!(output, ":asdf:\n\ncontent\n");
    }

    #[test]
    fn test_bad_endif_directive() {
        let input = "ifdef::asdf[]
content
endif::another[]";
        let output = Preprocessor.process(input);
        assert!(matches!(output, Err(Error::InvalidConditionalDirective)));
    }
}
