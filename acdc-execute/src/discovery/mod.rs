//! Discover executable commands from parsed `AsciiDoc` source.

mod error;

pub use error::DiscoveryError;

use acdc_converters_core::{InlineTextTransform, TraversalContext, visitor::Visitor};
use acdc_parser::{
    AttributeValue, Block, BlockMetadata, DelimitedBlockType, InlineNode, ParseResult, Section,
    SourceLocation, Substitution, VERBATIM, substitute_attributes,
};

use crate::command::{CommandBlock, CommandGraph, CommandGraphBuilder, CommandId};

impl TryFrom<&ParseResult> for CommandGraph {
    type Error = DiscoveryError;

    fn try_from(parsed: &ParseResult) -> Result<Self, Self::Error> {
        validate_source(parsed)?;
        let document = parsed.document();
        let mut traversal = TraversalContext::new(&document.attributes);
        let mut collector = CommandCollector {
            parsed,
            builder: CommandGraphBuilder::new(),
            section_title: None,
        };
        collector.visit_document(&mut traversal, document)?;
        Ok(collector.builder.build()?)
    }
}

fn validate_source(parsed: &ParseResult) -> Result<(), DiscoveryError> {
    if let Some(warning) = parsed.source_recovery() {
        return Err(DiscoveryError::RecoveredSource {
            source: warning.kind.clone(),
            location: warning.source_location().cloned().map(Box::new),
        });
    }
    Ok(())
}

struct CommandCollector<'doc> {
    parsed: &'doc ParseResult,
    builder: CommandGraphBuilder,
    section_title: Option<&'doc [InlineNode<'doc>]>,
}

impl<'doc> Visitor<'doc> for CommandCollector<'doc> {
    type Error = DiscoveryError;

    fn before_block(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        block: &'doc Block<'doc>,
    ) -> Result<(), Self::Error> {
        if let Some(metadata) = block.metadata()
            && metadata.roles.contains(&"command")
        {
            let (mut command, dependencies) = parse_command_block(
                block,
                metadata,
                self.parsed.source_location(block.location()),
                traversal,
            )?;
            command.metadata.section_title = self.section_title.map(|title| {
                InlineTextTransform::default()
                    .decode_char_refs(true)
                    .references(&self.parsed.document().references)
                    .to_string(title)
            });
            self.builder.add(command, dependencies);
        }
        Ok(())
    }

    fn visit_section(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        section: &'doc Section<'doc>,
    ) -> Result<(), Self::Error> {
        let parent = self.section_title.replace(&section.title);
        let result = traversal.visit_blocks(self, &section.content);
        self.section_title = parent;
        result
    }

    fn visit_inline_nodes(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _nodes: &[InlineNode<'_>],
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn parse_command_block(
    block: &Block<'_>,
    metadata: &BlockMetadata<'_>,
    location: SourceLocation,
    traversal: &TraversalContext<'_>,
) -> Result<(CommandBlock, Vec<CommandId>), DiscoveryError> {
    let anchor = metadata
        .id
        .as_ref()
        .ok_or_else(|| DiscoveryError::MissingId {
            location: Box::new(location.clone()),
        })?;
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only script block variants are accepted"
    )]
    let source_text = match block {
        Block::DelimitedBlock(block)
            if matches!(block.inner, DelimitedBlockType::DelimitedListing(_)) =>
        {
            block.source_text()
        }
        Block::Paragraph(paragraph) if matches!(metadata.style, Some("source" | "listing")) => {
            paragraph.source_text()
        }
        _ => {
            return Err(DiscoveryError::NotAScript {
                id: anchor.id.to_owned(),
                location: Box::new(location),
            });
        }
    }
    .ok_or_else(|| DiscoveryError::MissingSource {
        id: anchor.id.to_owned(),
        location: Box::new(location.clone()),
    })?;

    let invalid_id = |source| DiscoveryError::InvalidId {
        command: anchor.id.to_owned(),
        source,
        location: Box::new(location.clone()),
    };
    let id = anchor.id.parse().map_err(invalid_id)?;
    let dependencies = metadata
        .attributes
        .get_string("deps")
        .map(|dependencies| {
            dependencies
                .split(',')
                .map(str::trim)
                .filter(|dependency| !dependency.is_empty())
                .map(str::parse)
                .collect::<Result<Vec<_>, _>>()
                .map_err(invalid_id)
        })
        .transpose()?
        .unwrap_or_default();
    let interpreter =
        source_interpreter(metadata).map_err(|()| DiscoveryError::InvalidInterpreter {
            id: anchor.id.to_owned(),
            location: Box::new(location.clone()),
        })?;
    let description = metadata
        .attributes
        .get_string("description")
        .map(std::borrow::Cow::into_owned);
    let script = if metadata.uses_substitution(&Substitution::Attributes, VERBATIM) {
        prepare_script(source_text, traversal, anchor.id, &location)?
    } else {
        source_text.to_owned()
    };
    Ok((
        CommandBlock::new(id, script, interpreter, location).with_description(description),
        dependencies,
    ))
}

fn prepare_script(
    source: &str,
    traversal: &TraversalContext<'_>,
    id: &str,
    location: &SourceLocation,
) -> Result<String, DiscoveryError> {
    let mut missing = None;
    let script = substitute_attributes(source, |name| {
        let value = traversal.get(name);
        if value.is_none() && missing.is_none() {
            missing = Some(name.to_owned());
        }
        value
    });
    if let Some(name) = missing {
        return Err(DiscoveryError::MissingAttribute {
            id: id.to_owned(),
            name,
            location: Box::new(location.clone()),
        });
    }
    Ok(script.into_owned())
}

fn source_interpreter(metadata: &BlockMetadata<'_>) -> Result<Option<String>, ()> {
    let interpreter = match metadata.attributes.get("interpreter") {
        Some(AttributeValue::String(value)) => Some(value.to_string()),
        Some(_) => return Err(()),
        None if metadata.style == Some("source") => metadata
            .attributes
            .get_string("language")
            .map(std::borrow::Cow::into_owned),
        None => None,
    };
    if interpreter
        .as_ref()
        .is_some_and(|value| value.trim().is_empty() || value.contains('\0'))
    {
        return Err(());
    }
    Ok(interpreter)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    //! Unit tests for discovery: building a [`CommandGraph`] from a parsed `AsciiDoc` document.

    use acdc_parser::{Options, ParseResult, SafeMode};
    use rstest::rstest;

    use crate::{
        DiscoveryError,
        command::{BuildError, CommandBlock, CommandGraph},
    };

    // --------------------------------------------------------------------------
    // Helpers
    // --------------------------------------------------------------------------

    fn parse(src: &str) -> ParseResult {
        acdc_parser::parse(src, &Options::default()).unwrap_or_else(|e| panic!("parse failed: {e}"))
    }

    fn try_graph(src: &str) -> Result<CommandGraph, DiscoveryError> {
        CommandGraph::try_from(&parse(src))
    }

    fn graph(src: &str) -> CommandGraph {
        try_graph(src).unwrap_or_else(|e| panic!("graph should build: {e}"))
    }

    fn err(src: &str) -> DiscoveryError {
        try_graph(src).expect_err("graph build should fail")
    }

    /// Command ids in execution (topological) order.
    fn ids(built: &CommandGraph) -> Vec<String> {
        built
            .plan_all()
            .commands()
            .map(|b| b.metadata.id.as_str().to_string())
            .collect()
    }

    /// The single command block with `id`, failing the test if absent.
    fn find(built: &CommandGraph, id: &str) -> CommandBlock {
        built
            .plan_all()
            .commands()
            .find(|b| b.metadata.id.as_str() == id)
            .cloned()
            .unwrap_or_else(|| panic!("command {id:?} not found"))
    }

    /// Index of `name` within an ordered id list.
    fn pos(ids: &[String], name: &str) -> usize {
        ids.iter()
            .position(|s| s == name)
            .unwrap_or_else(|| panic!("{name:?} missing from {ids:?}"))
    }

    /// Render a single command block. `deps` and `lang` are optional; `body` is the script.
    fn cmd(id: &str, deps: Option<&str>, lang: Option<&str>, body: &str) -> String {
        let deps = deps.map(|d| format!(", deps=\"{d}\"")).unwrap_or_default();
        let lang = lang.map(|l| format!("[source, {l}]\n")).unwrap_or_default();
        format!("[.command, id={id}{deps}]\n{lang}----\n{body}\n----\n")
    }

    /// Wrap `body` in a level-1 section titled `name`.
    fn section(name: &str, body: &str) -> String {
        format!("== {name}\n\n{body}")
    }

    // --------------------------------------------------------------------------
    // Discovery: where command blocks are found
    // --------------------------------------------------------------------------

    #[test]
    fn empty_document_yields_empty_graph() {
        assert_eq!(ids(&graph("= Doc\n")), Vec::<String>::new());
    }

    #[test]
    fn document_with_no_commands_yields_empty_graph() {
        let src = "= Doc\n\nSome prose.\n\n[source, bash]\n----\necho not-a-command\n----\n";
        assert_eq!(ids(&graph(src)), Vec::<String>::new());
    }

    #[rstest]
    #[case::top_level(cmd("build", None, None, "echo hi"))]
    #[case::open_block(format!("--\n{}--\n", cmd("build", None, None, "echo hi")))]
    #[case::source_paragraph("[source,bash,role=command,id=build]\necho hi".into())]
    #[case::listing_paragraph("[listing,role=command,id=build]\necho hi".into())]
    #[case::nested_in_section(format!(
    "= Doc\n\n{}",
    section("Build", &cmd("build", None, None, "echo hi"))
))]
    #[case::nested_subsection(format!(
    "= Doc\n\n== Top\n\n{}",
    section("Build", &cmd("build", None, None, "echo hi"))
))]
    #[case::example_block(format!("= Doc\n\n====\n{}====\n", cmd("build", None, None, "echo hi")))]
    #[case::sidebar_block(format!("= Doc\n\n****\n{}****\n", cmd("build", None, None, "echo hi")))]
    #[case::quote_block(format!("= Doc\n\n____\n{}____\n", cmd("build", None, None, "echo hi")))]
    #[case::admonition_block(format!(
    "= Doc\n\n[NOTE]\n====\n{}====\n",
    cmd("build", None, None, "echo hi")
))]
    #[case::ordered_list_item(format!("= Doc\n\n. Build it\n+\n{}", cmd("build", None, None, "echo hi")))]
    #[case::unordered_list_item(format!("= Doc\n\n* Build it\n+\n{}", cmd("build", None, None, "echo hi")))]
    #[case::description_list_item(format!(
    "= Doc\n\nBuild:: Compile the project\n+\n{}",
    cmd("build", None, None, "echo hi")
))]
    fn single_command_is_discovered(#[case] src: String) {
        assert_eq!(ids(&graph(&src)), ["build"]);
    }

    #[test]
    fn commands_across_multiple_sections_are_all_found() {
        let src = format!(
            "= Doc\n\n{}\n{}",
            section("Build", &cmd("build", None, None, "echo build")),
            section("Test", &cmd("test", Some("build"), None, "echo test")),
        );
        let ids = ids(&graph(&src));
        assert_eq!(ids.len(), 2);
        assert!(pos(&ids, "build") < pos(&ids, "test"));
    }

    #[test]
    fn commands_keep_the_nearest_section_title_as_plain_text() {
        let built = graph(&format!(
            concat!(
                "= Commands\n:project: acdc\n\n{}\n",
                "== *Build* {{project}}{{apos}}s tools\n\n{}\n",
                "=== _Unit_ tests\n\n--\n{}--\n\n",
                "== Deploy\n\n{}\n",
                "[discrete]\n=== A separate heading\n\n{}\n",
            ),
            cmd("preamble", None, None, "true"),
            cmd("build", None, None, "true"),
            cmd("test", None, None, "true"),
            cmd("deploy", None, None, "true"),
            cmd("after-discrete", None, None, "true"),
        ));
        assert_eq!(find(&built, "preamble").metadata.section_title, None);
        assert_eq!(
            find(&built, "build").metadata.section_title.as_deref(),
            Some("Build acdc's tools")
        );
        assert_eq!(
            find(&built, "test").metadata.section_title.as_deref(),
            Some("Unit tests")
        );
        for id in ["deploy", "after-discrete"] {
            assert_eq!(
                find(&built, id).metadata.section_title.as_deref(),
                Some("Deploy")
            );
        }
    }

    #[rstest]
    #[case(None)]
    #[case(Some("Outer section"))]
    fn table_cell_sections_restore_the_enclosing_section(#[case] parent: Option<&str>) {
        let body = format!(
            "[cols=a]\n|===\na|== Cell section\n\n{}\n|===\n\n{}",
            cmd("cell", None, None, "true"),
            cmd("after", None, None, "true"),
        );
        let source = parent.map_or_else(|| body.clone(), |title| section(title, &body));
        let built = graph(&format!("= Commands\n\n{source}"));
        assert_eq!(
            find(&built, "cell").metadata.section_title.as_deref(),
            Some("Cell section")
        );
        assert_eq!(
            find(&built, "after").metadata.section_title.as_deref(),
            parent
        );
    }

    #[test]
    fn commands_across_distinct_list_items_are_all_found() {
        let src = format!(
            "= Doc\n\n. First\n+\n{}\n. Second\n+\n{}",
            cmd("build", None, None, "echo build"),
            cmd("test", Some("build"), None, "echo test"),
        );
        let ids = ids(&graph(&src));
        assert_eq!(ids.len(), 2);
        assert!(pos(&ids, "build") < pos(&ids, "test"));
    }

    // --------------------------------------------------------------------------
    // Non-commands are ignored
    // --------------------------------------------------------------------------

    #[test]
    fn listing_without_command_role_is_ignored() {
        let src = "[source, bash]\n----\necho hi\n----\n";
        assert_eq!(ids(&graph(src)), [] as [String; 0]);
    }

    #[test]
    fn command_role_without_id_errors() {
        // A `command` block with no id is a typo, not a no-op: surface it.
        let src = "[.command]\n----\necho hi\n----\n";
        assert!(matches!(err(src), DiscoveryError::MissingId { .. }));
    }

    #[test]
    fn command_role_on_non_listing_block_errors() {
        // An example block carrying the command role is not a script; it cannot be run.
        let src = "[.command, id=x]\n====\nsome content\n====\n";
        assert!(matches!(err(src), DiscoveryError::NotAScript { ref id, .. } if id == "x"));
    }

    #[test]
    fn non_script_paragraph_with_command_role_is_an_error() {
        assert!(matches!(
            err("[.command,id=prose]\nThis is a paragraph.\n"),
            DiscoveryError::NotAScript { .. }
        ));
        assert!(matches!(
            err("[.command]\nThis is a paragraph.\n"),
            DiscoveryError::MissingId { .. }
        ));
    }

    // --------------------------------------------------------------------------
    // Discovery across included files
    // --------------------------------------------------------------------------

    #[test]
    fn command_in_included_file_is_discovered() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let main = dir.path().join("main.adoc");
        let included = dir.path().join("commands.adoc");
        std::fs::write(&main, "= Doc\n\n== Build\n\ninclude::commands.adoc[]\n")
            .unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&included, cmd("build", None, None, "echo hi"))
            .unwrap_or_else(|e| panic!("{e}"));

        let parsed = acdc_parser::parse_file(
            &main,
            &Options::builder()
                .with_safe_mode(acdc_parser::SafeMode::Unsafe)
                .build()
                .unwrap(),
        )
        .unwrap_or_else(|e| panic!("parse_file failed: {e}"));
        let built = CommandGraph::try_from(&parsed).unwrap_or_else(|e| panic!("{e}"));

        assert_eq!(ids(&built), ["build"]);
        assert_eq!(
            find(&built, "build").metadata.section_title.as_deref(),
            Some("Build")
        );
    }

    // --------------------------------------------------------------------------
    // Shell / language
    // --------------------------------------------------------------------------

    #[test]
    fn default_interpreter_is_sh_when_no_language() {
        let block = find(&graph(&cmd("build", None, None, "echo hi")), "build");
        assert_eq!(block.metadata.interpreter, "sh");
    }

    #[test]
    fn source_block_with_no_language_defaults_to_sh() {
        // `[source]` with no language: style=="source" but no None-valued attribute key.
        // Different code path from having no [source,...] annotation at all.
        let src = "[.command, id=build]\n[source]\n----\necho hi\n----\n";
        let block = find(&graph(src), "build");
        assert_eq!(block.metadata.interpreter, "sh");
    }

    #[test]
    fn source_interpreter_ignores_block_options() {
        let src = "[.command, id=build]\n[source, bash, %linenums]\n----\necho hi\n----\n";
        let block = find(&graph(src), "build");
        assert_eq!(block.metadata.interpreter, "bash");
    }

    #[rstest]
    #[case("bash")]
    #[case("zsh")]
    #[case("python3")]
    #[case("fish")]
    fn source_interpreter_sets_interpreter(#[case] lang: &str) {
        let block = find(&graph(&cmd("build", None, Some(lang), "echo hi")), "build");
        assert_eq!(block.metadata.interpreter, lang);
    }

    // --------------------------------------------------------------------------
    // Script body
    // --------------------------------------------------------------------------

    #[rstest]
    #[case::single_line("cargo build", "cargo build\n")]
    #[case::multiline("set -e\ncargo build", "set -e\ncargo build\n")]
    fn script_body_is_captured(#[case] body: &str, #[case] expected: &str) {
        let block = find(&graph(&cmd("build", None, None, body)), "build");
        assert_eq!(block.script, expected);
    }

    #[test]
    fn script_location_reports_the_block_line() {
        // The command block opens on line 3 (after the title and a blank line).
        let src = "= Doc\n\n[.command, id=build]\n----\necho hi\n----\n";
        let block = find(&graph(src), "build");
        assert_eq!(block.location.location.start.line, 3);
    }

    // --------------------------------------------------------------------------
    // Dependencies
    // --------------------------------------------------------------------------

    #[test]
    fn dependency_orders_prerequisite_first() {
        let src = format!(
            "{}\n{}",
            cmd("build", None, None, "echo build"),
            cmd("test", Some("build"), None, "echo test"),
        );
        assert_eq!(ids(&graph(&src)), ["build", "test"]);
    }

    #[test]
    fn forward_referenced_dependency_resolves() {
        // `test` is declared *before* the `build` it depends on.
        let src = format!(
            "{}\n{}",
            cmd("test", Some("build"), None, "echo test"),
            cmd("build", None, None, "echo build"),
        );
        assert_eq!(ids(&graph(&src)), ["build", "test"]);
    }

    #[test]
    fn multiple_dependencies_all_precede_dependent() {
        let src = format!(
            "{}\n{}\n{}",
            cmd("a", None, None, "echo a"),
            cmd("b", None, None, "echo b"),
            cmd("c", Some("a, b"), None, "echo c"),
        );
        let ids = ids(&graph(&src));
        assert!(pos(&ids, "a") < pos(&ids, "c"));
        assert!(pos(&ids, "b") < pos(&ids, "c"));
    }

    #[rstest]
    #[case("build", &["build"])]
    #[case("a,b", &["a", "b"])]
    #[case("a, b", &["a", "b"])]
    #[case("  a ,  b  ", &["a", "b"])]
    #[case("a, b, c", &["a", "b", "c"])]
    fn deps_attribute_is_split_and_trimmed(#[case] deps: &str, #[case] expected: &[&str]) {
        // Provide every named prerequisite so the graph resolves, then check ordering.
        let mut src = String::new();
        for dep in expected {
            src.push_str(&cmd(dep, None, None, "echo dep"));
            src.push('\n');
        }
        src.push_str(&cmd("target", Some(deps), None, "echo target"));

        let ids = ids(&graph(&src));
        for dep in expected {
            assert!(
                pos(&ids, dep) < pos(&ids, "target"),
                "{dep} should precede target"
            );
        }
    }

    #[test]
    fn trailing_and_empty_dep_segments_are_ignored() {
        let src = format!(
            "{}\n{}",
            cmd("build", None, None, "echo build"),
            cmd("test", Some("build, ,"), None, "echo test"),
        );
        assert_eq!(ids(&graph(&src)), ["build", "test"]);
    }

    #[test]
    fn empty_deps_attribute_yields_no_dependencies() {
        // deps="" splits to [""], filtered to nothing — command has no prerequisites.
        let src = cmd("build", Some(""), None, "echo build");
        assert_eq!(ids(&graph(&src)), ["build"]);
    }

    // --------------------------------------------------------------------------
    // Errors
    // --------------------------------------------------------------------------

    #[test]
    fn invalid_dependency_id_errors() {
        let src = format!(
            "{}\n{}",
            cmd("build", None, None, "echo build"),
            cmd("test", Some("bad id"), None, "echo test"),
        );
        assert!(matches!(err(&src), DiscoveryError::InvalidId { .. }));
    }

    #[test]
    fn invalid_command_id_errors() {
        let src = "[.command, id=bad.id]\n----\necho hi\n----\n";
        assert!(matches!(err(src), DiscoveryError::InvalidId { .. }));
    }

    #[test]
    fn duplicate_command_id_errors() {
        let src = format!(
            "{}\n{}",
            cmd("build", None, None, "echo one"),
            cmd("build", None, None, "echo two"),
        );
        assert!(matches!(
            err(&src),
            DiscoveryError::Build(BuildError::DuplicateId { .. })
        ));
    }

    #[test]
    fn unknown_dependency_errors() {
        let src = cmd("test", Some("missing"), None, "echo test");
        assert!(matches!(
            err(&src),
            DiscoveryError::Build(BuildError::UnknownDep { .. })
        ));
    }

    #[test]
    fn self_dependency_errors() {
        let src = cmd("loop", Some("loop"), None, "echo loop");
        assert!(matches!(
            err(&src),
            DiscoveryError::Build(BuildError::Cycle { .. })
        ));
    }

    #[test]
    fn dependency_cycle_errors() {
        let src = format!(
            "{}\n{}",
            cmd("a", Some("b"), None, "echo a"),
            cmd("b", Some("a"), None, "echo b"),
        );
        assert!(matches!(
            err(&src),
            DiscoveryError::Build(BuildError::Cycle { .. })
        ));
    }

    #[test]
    fn missing_id_error_reports_the_block_line() {
        let src = "= Doc\n\n[.command]\n----\necho hi\n----\n";
        let DiscoveryError::MissingId { location } = err(src) else {
            panic!("expected MissingId")
        };
        assert_eq!(location.location.start.line, 3);
    }

    #[test]
    fn not_a_script_error_reports_the_block_line() {
        let src = "= Doc\n\n[.command, id=x]\n====\nsome content\n====\n";
        let DiscoveryError::NotAScript { location, .. } = err(src) else {
            panic!("expected NotAScript")
        };
        assert_eq!(location.location.start.line, 3);
    }

    // --------------------------------------------------------------------------
    // Safe mode is a document-read policy, not a command sandbox
    // --------------------------------------------------------------------------

    #[test]
    fn safe_mode_still_discovers_commands() {
        // Commands are discovered and validated identically under SECURE safe mode;
        // safe mode only limits what the document may include.
        let options = Options::builder()
            .with_safe_mode(SafeMode::Secure)
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        let src = "[.command, id=build]\n----\necho hi\n----\n";
        let parsed = acdc_parser::parse(src, &options).unwrap_or_else(|e| panic!("{e}"));
        let built = CommandGraph::try_from(&parsed).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ids(&built), ["build"]);
    }

    // --------------------------------------------------------------------------
    // End-to-end example
    // --------------------------------------------------------------------------

    #[test]
    fn readme_example_builds_expected_graph() {
        let src = "= My Project\n\n== Build\n\n[.command, id=build]\n[source, bash]\n----\ncargo xtask build\n----\n\n== Tests\n\n[.command, id=test, deps=\"build\"]\n[source, bash]\n----\ncargo nextest run\n----\n";
        let built = graph(src);
        let plan = built.plan_all();
        let blocks: Vec<&CommandBlock> = plan.commands().collect();

        let names: Vec<&str> = blocks.iter().map(|b| b.metadata.id.as_str()).collect();
        assert_eq!(names, ["build", "test"]);
        let first = blocks.first().unwrap_or_else(|| panic!("build missing"));
        let second = blocks.get(1).unwrap_or_else(|| panic!("test missing"));
        assert_eq!(first.metadata.interpreter, "bash");
        assert_eq!(first.script, "cargo xtask build\n");
        assert_eq!(second.script, "cargo nextest run\n");
    }

    #[rstest]
    #[case("cat <<'EOF'\n<1>\nEOF\n")]
    #[case("printf '%s' \\<1>\n")]
    #[case("<!--1-->\n<.>\n")]
    #[case("echo '{value}' *bold*\n\n")]
    fn scripts_preserve_literal_body(#[case] body: &str) {
        let source = format!(
            ":value: replaced\n\n[source,bash,role=command,id=literal]\n----\n{body}----\n"
        );
        assert_eq!(find(&graph(&source), "literal").script, body);
    }

    #[test]
    fn substitution_follows_the_parser_feature_even_when_features_are_unified() {
        let mut parsed = parse(
            ":value: expanded\n\n[.command,id=run,subs=attributes]\n----\nprintf '{value}'\n----\n",
        );
        let unsupported = parsed.source_recovery().is_some();
        parsed.take_warnings();
        let result = CommandGraph::try_from(&parsed);
        if unsupported {
            assert!(matches!(
                result,
                Err(DiscoveryError::RecoveredSource { .. })
            ));
        } else {
            assert_eq!(find(&result.unwrap(), "run").script, "printf 'expanded'\n");
        }
    }

    #[cfg(feature = "pre-spec-subs")]
    mod attribute_substitution {
        use super::*;

        fn command(id: &str, subs: &str, body: &str) -> String {
            format!("[source,sh,role=command,id={id},subs=\"{subs}\"]\n----\n{body}----\n")
        }

        #[rstest]
        #[case("attributes", "expanded")]
        #[case("+attributes", "expanded")]
        #[case("attributes+", "expanded")]
        #[case("normal", "expanded")]
        #[case("none", "{value}")]
        #[case("-attributes", "{value}")]
        #[case("verbatim", "{value}")]
        #[case("-attributes,+attributes", "expanded")]
        #[case("+attributes,-attributes", "{value}")]
        fn resolves_substitutions_against_verbatim_defaults(
            #[case] subs: &str,
            #[case] expected: &str,
        ) {
            let source = format!(":value: expanded\n\n{}", command("run", subs, "{value}\n"));
            assert_eq!(find(&graph(&source), "run").script, format!("{expected}\n"));
        }

        #[rstest]
        #[case("source,sh")]
        #[case("listing")]
        fn source_paragraphs_expand_attributes_without_adding_a_newline(#[case] style: &str) {
            let source = format!(
                ":value: expanded\n\n[{style},role=command,id=run,subs=attributes]\nprintf '{{value}}'"
            );
            assert_eq!(find(&graph(&source), "run").script, "printf 'expanded'");
        }

        #[test]
        fn enabled_attributes_leave_other_code_and_callouts_unchanged() {
            let body = "printf '*bold* _italic_ <1> <!--2--> -- ... {value}'\n\n  \n";
            let source = format!(":value: expanded\n\n{}", command("run", "normal", body));
            assert_eq!(
                find(&graph(&source), "run").script,
                "printf '*bold* _italic_ <1> <!--2--> -- ... expanded'\n\n\n"
            );
        }

        #[test]
        fn escaped_references_are_literal_and_do_not_require_a_value() {
            let source = format!(
                ":value: expanded\n\n{}",
                command("run", "attributes", "\\{value} \\{missing} {value}\n")
            );
            assert_eq!(
                find(&graph(&source), "run").script,
                "{value} {missing} expanded\n"
            );
        }

        #[test]
        fn malformed_references_remain_literal() {
            let source = command("run", "attributes", "{} {not a name} {missing\n");
            assert_eq!(
                find(&graph(&source), "run").script,
                "{} {not a name} {missing\n"
            );
        }

        #[test]
        fn first_missing_attribute_is_a_located_preflight_error() {
            let source = command("run", "attributes", "{first} {second}\n");
            let error = err(&source);
            assert!(
                matches!(&error, DiscoveryError::MissingAttribute { id, name, .. } if id == "run" && name == "first")
            );
            assert!(error.source_location().is_some());
            assert!(error.related_location().is_none());
            assert_eq!(
                error.to_string(),
                "command `run` references missing document attribute `first`"
            );
        }

        #[test]
        fn presence_attributes_expand_to_empty_text() {
            let source = format!(
                ":empty:\n\n{}",
                command("run", "attributes", "before{empty}after\n")
            );
            assert_eq!(find(&graph(&source), "run").script, "beforeafter\n");
        }

        #[test]
        fn unset_attributes_mask_header_values() {
            let source = format!(
                ":value: header\n\nBody.\n\n:value!:\n\n{}",
                command("run", "attributes", "{value}\n")
            );
            assert!(
                matches!(err(&source), DiscoveryError::MissingAttribute { name, .. } if name == "value")
            );
        }

        #[test]
        fn disabled_attributes_do_not_reject_missing_references() {
            let source = command("run", "none", "{missing} \\{also-missing}\n");
            assert_eq!(
                find(&graph(&source), "run").script,
                "{missing} \\{also-missing}\n"
            );
        }

        #[test]
        fn substituted_values_are_not_expanded_again() {
            let options = Options::with_attributes([("value", "{missing}")]).unwrap();
            let source = command("run", "attributes", "{value}\n");
            let parsed = acdc_parser::parse(&source, &options).unwrap();
            assert_eq!(
                find(&CommandGraph::try_from(&parsed).unwrap(), "run").script,
                "{missing}\n"
            );
        }

        #[test]
        fn source_order_attributes_are_frozen_before_dependency_ordering() {
            let source = concat!(
                "= Commands\n:value: header\n\n",
                "[.command,id=first,deps=second,subs=attributes]\n----\n{value}\n----\n\n",
                ":value: later\n\n",
                "[.command,id=second,subs=attributes]\n----\n{value}\n----\n",
            );
            let graph = graph(source);
            assert_eq!(ids(&graph), ["second", "first"]);
            assert_eq!(find(&graph, "first").script, "header\n");
            assert_eq!(find(&graph, "second").script, "later\n");
        }

        #[rstest]
        #[case::inherited_value_is_locked("value", "outer\n")]
        #[case::local_value_is_accepted("local", "cell\n")]
        fn table_cell_attributes_obey_inheritance_and_scope(
            #[case] name: &str,
            #[case] expected: &str,
        ) {
            let source = format!(
                concat!(
                    "= Commands\n:value: outer\n\n[cols=\"a,a\"]\n|===\n",
                    "|\nCell one.\n\n:{name}: cell\n\n",
                    "[.command,id=cell,subs=attributes]\n----\n{{{name}}}\n----\n",
                    "|\n[.command,id=sibling,subs=attributes]\n----\n{{value}}\n----\n",
                    "|===\n\n",
                    "[.command,id=after,subs=attributes]\n----\n{{value}}\n----\n",
                ),
                name = name
            );
            let graph = graph(&source);
            assert_eq!(find(&graph, "cell").script, expected);
            assert_eq!(find(&graph, "sibling").script, "outer\n");
            assert_eq!(find(&graph, "after").script, "outer\n");
        }

        #[rstest]
        #[case::sibling(true)]
        #[case::parent(false)]
        fn local_table_attributes_are_missing_outside_their_cell(#[case] sibling: bool) {
            let cell = command("inside", "attributes", "{local}\n");
            let outside = command("outside", "attributes", "{local}\n");
            let mut source = format!("[cols=a]\n|===\n|\n:local: cell\n\n{cell}\n");
            source.push_str(if sibling { "|\n" } else { "|===\n\n" });
            source.push_str(&outside);
            if sibling {
                source.push_str("|===\n");
            }
            assert!(
                matches!(err(&source), DiscoveryError::MissingAttribute { id, name, .. }
            if id == "outside" && name == "local")
            );
        }

        #[test]
        fn table_cells_use_their_initial_document_attributes() {
            let source = concat!(
                "= Commands\n:doctype: book\n\n[cols=a]\n|===\n|\n",
                "[.command,id=cell,subs=attributes]\n----\n{doctype}\n----\n|===\n\n",
                "[.command,id=after,subs=attributes]\n----\n{doctype}\n----\n",
            );
            let graph = graph(source);
            assert_eq!(find(&graph, "cell").script, "article\n");
            assert_eq!(find(&graph, "after").script, "book\n");
        }

        #[test]
        fn included_attribute_events_apply_in_source_order() {
            let directory = tempfile::tempdir().unwrap();
            let included = directory.path().join("included.adoc");
            std::fs::write(
                &included,
                format!(
                    ":value: included\n\n{}",
                    command("included", "attributes", "{value}\n")
                ),
            )
            .unwrap();
            let main = directory.path().join("main.adoc");
            std::fs::write(
                &main,
                concat!(
                    "= Commands\n:value: initial\n\nBody.\n\ninclude::included.adoc[]\n\n",
                    "[.command,id=after,subs=attributes]\n----\n{value}\n----\n",
                ),
            )
            .unwrap();
            let parsed = acdc_parser::parse_file(
                &main,
                &Options::builder()
                    .with_safe_mode(acdc_parser::SafeMode::Unsafe)
                    .build()
                    .unwrap(),
            )
            .unwrap();
            let graph = CommandGraph::try_from(&parsed).unwrap();
            let included_command = find(&graph, "included");
            assert_eq!(included_command.script, "included\n");
            assert_eq!(
                included_command.location.file.as_deref(),
                Some(included.as_path())
            );
            assert_eq!(find(&graph, "after").script, "included\n");
        }

        #[test]
        fn drained_warnings_do_not_change_missing_attribute_validation() {
            let mut parsed = parse(&command("run", "attributes", "{missing}\n"));
            parsed.take_warnings();
            assert!(
                matches!(CommandGraph::try_from(&parsed), Err(DiscoveryError::MissingAttribute { name, .. }) if name == "missing")
            );
        }
    }

    #[test]
    fn source_paragraph_preserves_missing_final_newline() {
        let source = "[source,bash,role=command,id=literal]\nprintf '%s' '<1>'";
        assert_eq!(find(&graph(source), "literal").script, "printf '%s' '<1>'");
    }

    #[test]
    fn table_commands_respect_header_cell_semantics() {
        let source = format!(
            "[cols=a,options=\"header,footer\"]\n|===\na|{}\na|{}\na|{}\n|===\n",
            cmd("header", None, None, "echo header"),
            cmd("body", None, None, "echo body"),
            cmd("footer", None, None, "echo footer"),
        );
        // Header cells use inline content even with an explicit AsciiDoc cell style.
        assert_eq!(ids(&graph(&source)), ["body", "footer"]);
    }

    #[test]
    fn marked_media_is_an_error() {
        assert!(matches!(
            err("[.command,id=image]\nimage::image.png[]"),
            DiscoveryError::NotAScript { .. }
        ));
    }

    #[rstest]
    #[case("[.command,id=broken]\n----\necho hi")]
    #[case("[cols=\"1,1\"]\n|===\n|only one cell\n|===")]
    #[case("|===\n|unterminated")]
    #[case("ifdef::missing[]\nremaining content")]
    fn recovered_documents_cannot_build_command_graphs(#[case] broken: &str) {
        let source = format!("{}\n{broken}", cmd("valid", None, None, "echo valid"));
        assert!(matches!(
            err(&source),
            DiscoveryError::RecoveredSource { .. }
        ));
    }

    #[test]
    fn presentation_warnings_do_not_block_commands() {
        let source = format!(
            "See <<missing>>.\n\n{}",
            cmd("valid", None, None, "echo valid")
        );
        let mut parsed = parse(&source);
        assert!(parsed.warnings().iter().any(|warning| matches!(
            warning.kind,
            acdc_parser::WarningKind::UnresolvedReference { .. }
        )));
        assert_ne!(parsed.take_warnings(), []);
        assert_eq!(ids(&CommandGraph::try_from(&parsed).unwrap()), ["valid"]);
    }

    #[test]
    fn routing_warnings_does_not_permit_recovered_commands() {
        let mut parsed = parse("[.command,id=broken]\n----\necho hi");
        assert_ne!(parsed.take_warnings(), []);
        assert_eq!(parsed.warnings(), []);
        assert!(matches!(
            CommandGraph::try_from(&parsed),
            Err(DiscoveryError::RecoveredSource { .. })
        ));
    }

    #[test]
    fn unclosed_included_conditionals_keep_the_original_opening_location() {
        let directory = tempfile::tempdir().unwrap();
        let main = directory.path().join("main.adoc");
        let included = directory.path().join("conditional.adoc");
        std::fs::write(&main, "include::conditional.adoc[lines=3..4]\n").unwrap();
        std::fs::write(&included, "ignored\n\nifdef::missing[]\nremaining\n").unwrap();
        let parsed = acdc_parser::parse_file(
            &main,
            &Options::builder()
                .with_safe_mode(acdc_parser::SafeMode::Unsafe)
                .build()
                .unwrap(),
        )
        .unwrap();
        let error = CommandGraph::try_from(&parsed).unwrap_err();
        let location = error.source_location().unwrap();
        assert_eq!(location.file.as_deref(), Some(included.as_path()));
        assert_eq!(location.location.start.line, 3);
    }

    #[rstest]
    #[case("interpreter=\"\"")]
    #[case("interpreter=\"   \"")]
    fn empty_interpreter_overrides_are_errors(#[case] attribute: &str) {
        let source = format!("[source,bash,role=command,id=run,{attribute}]\n----\necho hi\n----");
        assert!(matches!(
            err(&source),
            DiscoveryError::InvalidInterpreter { .. }
        ));
    }

    #[test]
    fn interpreter_override_is_a_single_executable_path() {
        let source = "[source,python,role=command,id=run,interpreter=\"/a path/python3\"]\n----\nprint('hi')\n----";
        assert_eq!(
            find(&graph(source), "run").metadata.interpreter,
            "/a path/python3"
        );
    }

    #[test]
    fn non_string_and_nul_interpreters_are_rejected() {
        for value in [
            acdc_parser::AttributeValue::Bool(true),
            acdc_parser::AttributeValue::String("bad\0name".into()),
        ] {
            let mut metadata = acdc_parser::BlockMetadata::default();
            metadata.attributes.insert("interpreter".into(), value);
            assert!(super::source_interpreter(&metadata).is_err());
        }
    }

    #[test]
    fn required_includes_block_graphs_but_absent_optional_includes_do_not() {
        let directory = tempfile::tempdir().unwrap();
        let options = Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .with_base_dir(directory.path())
            .build()
            .unwrap();
        for (attributes, blocked) in [("", true), ("opts=optional", false)] {
            let source = format!(
                "include::missing.adoc[{attributes}]\n\n{}",
                cmd("run", None, None, "echo hi")
            );
            let parsed = acdc_parser::parse(&source, &options).unwrap();
            let result = CommandGraph::try_from(&parsed);
            if blocked {
                assert!(matches!(
                    result,
                    Err(DiscoveryError::RecoveredSource { .. })
                ));
            } else {
                assert_eq!(ids(&result.unwrap()), ["run"]);
            }
        }
    }

    #[test]
    fn nested_selected_include_retains_script_and_original_location() {
        let directory = tempfile::tempdir().unwrap();
        let chapters = directory.path().join("chapters");
        std::fs::create_dir(&chapters).unwrap();
        let main = directory.path().join("main.adoc");
        let outer = chapters.join("outer.adoc");
        let scripts = directory.path().join("scripts.adoc");
        std::fs::write(&main, "include::chapters/outer.adoc[]").unwrap();
        std::fs::write(&outer, "include::../scripts.adoc[tag=command]").unwrap();
        std::fs::write(
            &scripts,
            format!(
                "ignored\n// tag::command[]\n{}// end::command[]\n",
                cmd("run", None, None, "cat <<'EOF'\n<1>\nEOF")
            ),
        )
        .unwrap();
        let parsed = acdc_parser::parse_file(
            &main,
            &Options::builder()
                .with_safe_mode(SafeMode::Safe)
                .build()
                .unwrap(),
        )
        .unwrap();
        let command = find(&CommandGraph::try_from(&parsed).unwrap(), "run");
        assert_eq!(command.script, "cat <<'EOF'\n<1>\nEOF\n");
        assert_eq!(command.location.file.as_deref(), Some(scripts.as_path()));
        assert_eq!(command.location.location.start.line, 3);
    }

    #[rstest]
    #[case("[.command]\n----\necho hi\n----", false)]
    #[case("[.command,id=bad.id]\n----\necho hi\n----", false)]
    #[case("[.command,id=run,deps=missing]\n----\necho hi\n----", true)]
    fn included_errors_keep_the_original_file(#[case] source: &str, #[case] graph_error: bool) {
        let directory = tempfile::tempdir().unwrap();
        let main = directory.path().join("main.adoc");
        let included = directory.path().join("included.adoc");
        std::fs::write(&main, "include::included.adoc[]").unwrap();
        std::fs::write(&included, format!("intro\n\n{source}")).unwrap();
        let parsed = acdc_parser::parse_file(
            &main,
            &Options::builder()
                .with_safe_mode(acdc_parser::SafeMode::Unsafe)
                .build()
                .unwrap(),
        )
        .unwrap();
        let error = CommandGraph::try_from(&parsed).unwrap_err();
        let location = error.source_location().unwrap();
        assert_eq!(location.file.as_deref(), Some(included.as_path()));
        assert_eq!(location.location.start.line, 3);
        assert_eq!(matches!(error, DiscoveryError::Build(_)), graph_error);
    }
}
