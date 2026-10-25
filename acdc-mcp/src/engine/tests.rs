use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use acdc_parser::AttributeValue;
use clap::Parser;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use crate::{
    config::{Cli, Config},
    engine::{Engine, FileRequest, FindRequest, ReadRequest, ResolveRequest},
};

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn configuration(roots: &[PathBuf], attributes: &[&str]) -> Config {
    let mut arguments = vec![OsString::from("acdc-mcp"), "--strictly-root-folders".into()];
    for root in roots {
        arguments.extend(["--root".into(), root.as_os_str().to_owned()]);
    }
    for attribute in attributes {
        arguments.extend(["-a".into(), OsString::from(attribute)]);
    }
    Cli::try_parse_from(arguments)
        .unwrap()
        .into_config()
        .unwrap()
}

fn engine(root: &Path) -> Engine {
    Engine::new(
        Arc::new(configuration(&[root.to_owned()], &[])),
        CancellationToken::new(),
    )
}

fn file(path: &str) -> FileRequest {
    serde_json::from_value(json!({"path":path})).unwrap()
}
fn read(path: &str, selector: &str) -> ReadRequest {
    serde_json::from_value(json!({"path":path,"selector":selector})).unwrap()
}
fn find(query: &str) -> FindRequest {
    serde_json::from_value(json!({"query":query})).unwrap()
}
fn records(response: &str) -> Vec<Value> {
    let page: Value = serde_json::from_str(response).unwrap();
    page["items"].as_array().unwrap().clone()
}
fn content(response: &str) -> String {
    records(response)
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect()
}

const GUIDE: &str = "= Widget Guide\nAda Lovelace <ada@example.com>\nv1.2, 2026-10-20: Stable\n:product: Widget\n:keywords: setup, onboarding\n\nIntroduction.\n\n[#install]\n== Installation\n\nInstall *{product}* with `cargo install widget`.\n\n=== Linux\n\nUse the package manager.\n\n.Bootstrap\n[#bootstrap]\n[source,sh]\n----\necho '*literal*'\n# not a section\n----\n\n.Package matrix\n[#matrix]\n|===\n|System |Package\n|Linux |widget\n|===\n\n== Configuration\n\nSee <<install>> and https://example.com[Website].\n";

#[test]
fn inline_images_preserve_alt_text_in_both_formats() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "images.adoc",
        "= Images\n\n== Pictures\n\nSee image:diagram.svg[Architecture overview] for details.\n",
    );
    let engine = engine(root.path());
    let mut request = read("images.adoc", "Pictures");
    let asciidoc = content(&engine.read(&request, true).unwrap());
    assert!(
        asciidoc.contains("image:diagram.svg[Architecture overview]"),
        "{asciidoc}"
    );
    request.format = crate::render::Format::Plain;
    let plain = content(&engine.read(&request, true).unwrap());
    assert!(
        plain.contains("Architecture overview (diagram.svg)"),
        "{plain}"
    );
}

#[test]
fn setext_headings_follow_the_server_feature() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "setext.adoc",
        "Guide\n=====\n\nInstallation\n------------\n\nInstall Widget.\n",
    );
    let tree = records(&engine(root.path()).tree(&file("setext.adoc")).unwrap());
    let has_installation = tree.iter().any(|item| item["title"] == "Installation");
    assert_eq!(has_installation, cfg!(feature = "setext"), "{tree:?}");
}

#[test]
fn section_reading_handles_descendants_formats_and_blocks() {
    let root = TempDir::new().unwrap();
    write(root.path(), "guide.adoc", GUIDE);
    let engine = engine(root.path());
    let tree = records(&engine.tree(&file("guide.adoc")).unwrap());
    assert!(tree.iter().any(|item| item["id"] == "install"));
    assert!(tree.iter().any(|item| item["title"] == "Bootstrap"));
    assert!(!tree.iter().any(|item| item["title"] == "not a section"));
    let mut request = read("guide.adoc", "install");
    let assembled = content(&engine.read(&request, true).unwrap());
    assert!(assembled.contains("=== Linux"));
    assert!(assembled.contains("**Widget**"), "{assembled}");
    assert!(!assembled.contains("Configuration"));
    request.include_subsections = false;
    let direct = content(&engine.read(&request, true).unwrap());
    assert!(!direct.contains("Linux"));
    request.include_subsections = true;
    request.format = crate::render::Format::Plain;
    let plain = content(&engine.read(&request, true).unwrap());
    assert!(
        plain.contains("Install Widget with cargo install widget"),
        "{plain}"
    );
    assert!(plain.contains("echo '*literal*'"));
    assert!(plain.contains("Linux | widget"));
    let code = content(
        &engine
            .read(&read("guide.adoc", "bootstrap"), false)
            .unwrap(),
    );
    assert!(code.contains("[source,sh]"), "{code}");
    assert!(code.contains("echo '*literal*'"));
    assert!(content(&engine.read(&read("guide.adoc", "matrix"), false).unwrap()).contains("|==="));
    assert!(
        content(&engine.read(&read("guide.adoc", "@preamble"), true).unwrap())
            .contains("Introduction.")
    );
}

#[test]
fn ambiguous_titles_and_ids_require_an_explicit_choice() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "guide.adoc",
        "= Guide\n\n== Setup\n\n=== Installation\n\nFirst.\n\n== Advanced\n\n=== Installation\n\nSecond.\n",
    );
    let engine = engine(root.path());
    let ambiguous = records(
        &engine
            .read(&read("guide.adoc", "Installation"), true)
            .unwrap(),
    );
    assert!(ambiguous.iter().any(|item| item["status"] == "ambiguous"));
    let exact = content(
        &engine
            .read(&read("guide.adoc", "Setup > Installation"), true)
            .unwrap(),
    );
    assert!(exact.contains("First."));
    assert!(!exact.contains("Second."));
    let candidate = ambiguous
        .iter()
        .find(|item| item["heading_path"] == json!(["Advanced", "Installation"]))
        .unwrap();
    let second = content(
        &engine
            .read(
                &read("guide.adoc", candidate["selector"].as_str().unwrap()),
                true,
            )
            .unwrap(),
    );
    assert!(second.contains("Second."));
    write(
        root.path(),
        "duplicate.adoc",
        "= Guide\n\n[#same]\n== One\n\nA.\n\n[#same]\n== Two\n\nB.\n",
    );
    let duplicate = records(&engine.read(&read("duplicate.adoc", "same"), true).unwrap());
    assert!(
        duplicate.iter().any(|item| item["status"] == "ambiguous"),
        "{duplicate:?}"
    );
}

#[test]
fn includes_are_assembled_and_retain_nested_original_locations() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "book.adoc",
        "= Book\n\ninclude::chapters/setup.adoc[leveloffset=+1]\n",
    );
    write(
        root.path(),
        "chapters/setup.adoc",
        "= Setup\n\ninclude::../parts/install.adoc[tags=install]\n",
    );
    write(
        root.path(),
        "parts/install.adoc",
        "Not selected.\n// tag::install[]\n== Installation\n\nInstall here.\n// end::install[]\n",
    );
    let engine = engine(root.path());
    let tree = records(&engine.tree(&file("book.adoc")).unwrap());
    let installation = tree
        .iter()
        .find(|item| item["title"] == "Installation")
        .unwrap();
    assert_eq!(
        installation["source"]["file"],
        root.path()
            .join("parts/install.adoc")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(installation["source"]["line"], 3);
    let text = content(
        &engine
            .read(&read("book.adoc", "Installation"), true)
            .unwrap(),
    );
    assert!(text.contains("Install here."));
    assert!(!text.contains("Not selected."));
    let independent = content(
        &engine
            .read(&read("parts/install.adoc", "@document"), true)
            .unwrap(),
    );
    assert!(independent.contains("Not selected."));
}

#[test]
fn request_attributes_accept_only_strings_and_booleans() {
    let attributes = json!({
        "product": "Widget",
        "empty": "",
        "literal": "false",
        "unicode": "café",
        "enabled": true,
        "disabled": false,
    });
    let request: FileRequest = serde_json::from_value(json!({
        "path": "guide.adoc",
        "attributes": attributes,
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(request).unwrap()["attributes"],
        attributes
    );
    assert!(file("guide.adoc").common.attributes.is_empty());
    for value in [json!(null), json!(42), json!(1.5), json!([]), json!({})] {
        assert!(
            serde_json::from_value::<FileRequest>(json!({
                "path": "guide.adoc",
                "attributes": {"invalid": value},
            }))
            .is_err(),
            "{value}"
        );
    }
}

#[test]
fn request_attributes_schema_allows_only_strings_and_booleans() {
    let schema = serde_json::to_value(rmcp::schemars::schema_for!(FileRequest)).unwrap();
    let attributes = &schema["properties"]["attributes"];
    assert_eq!(attributes["type"], "object");
    let values = &attributes["additionalProperties"];
    let values = values["$ref"].as_str().map_or(values, |reference| {
        schema
            .pointer(reference.strip_prefix('#').unwrap())
            .unwrap()
    });
    assert_eq!(
        values["anyOf"],
        json!([{"type": "string"}, {"type": "boolean"}])
    );
}

#[test]
fn attributes_override_startup_and_apply_at_their_source_position() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "conditional.adoc",
        "= Guide\n\n== Installation\n\nifdef::community[]\nCommunity edition.\nendif::[]\n\n:product: First\n\nUse {product}.\n\n:product: Second\n\nUse {product}.\n\nYour {audience}.\n",
    );
    let config = configuration(&[root.path().to_owned()], &["audience=startup"]);
    let engine = Engine::new(Arc::new(config), CancellationToken::new());
    let mut request = read("conditional.adoc", "Installation");
    request
        .common
        .attributes
        .insert("community".into(), AttributeValue::Bool(true));
    request
        .common
        .attributes
        .insert("audience".into(), AttributeValue::from("request"));
    let text = content(&engine.read(&request, true).unwrap());
    assert!(text.contains("Community edition."));
    assert!(text.contains("Use First."), "{text}");
    assert!(text.contains("Use Second."));
    assert!(text.contains("Your request."));
    request
        .common
        .attributes
        .insert("community".into(), AttributeValue::Bool(false));
    assert!(!content(&engine.read(&request, true).unwrap()).contains("Community edition."));
}

#[test]
fn structural_search_respects_gitignore_and_does_not_search_body_prose() {
    let root = TempDir::new().unwrap();
    write(root.path(), ".gitignore", "ignored/\n");
    write(
        root.path(),
        "ignored/secret.adoc",
        "= Hidden\n\n== Installation\n\nHidden.\n",
    );
    write(root.path(), "docs/guide.adoc", GUIDE);
    write(
        root.path(),
        "docs/network.adoc",
        "= Network\n\n== TransportLayerSecurity\n\nbodyonlymagic\n",
    );
    let engine = engine(root.path());
    let installation = records(&engine.find(&find("install")).unwrap());
    assert!(
        installation
            .iter()
            .any(|item| item["title"] == "Installation")
    );
    assert!(
        !installation
            .iter()
            .any(|item| item["path"].as_str().is_some_and(|p| p.contains("ignored")))
    );
    let body = records(&engine.find(&find("bodyonlymagic")).unwrap());
    assert!(!body.iter().any(|item| item["kind"] == "match"));
    assert!(
        records(&engine.find(&find("tls")).unwrap())
            .iter()
            .any(|item| item["title"] == "TransportLayerSecurity")
    );
    assert!(
        records(&engine.find(&find("onboarding")).unwrap())
            .iter()
            .any(|item| item["kind"] == "match")
    );
}

#[test]
fn fuzzy_search_matches_abbreviations_unicode_and_every_query_term() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "guide.adoc",
        "= Guide\n\n== Installation\n\nInstall here.\n\n== TransportLayerSecurity\n\nSecure transport.\n\n== Café setup\n\nCoffee.\n\n== Authorization\n\nAccess control.\n",
    );
    let engine = engine(root.path());
    for (query, title) in [
        ("tls", "TransportLayerSecurity"),
        ("cafe", "Café setup"),
        ("CAFÉ SET", "Café setup"),
        ("setup cafe", "Café setup"),
    ] {
        let result = records(&engine.find(&find(query)).unwrap());
        assert!(
            result.iter().any(|item| item["title"] == title),
            "{query}: {result:?}"
        );
    }
    for query in [
        "authentication",
        "installation missing",
        "find installation",
        "^installation$",
    ] {
        let result = records(&engine.find(&find(query)).unwrap());
        assert!(
            !result.iter().any(|item| item["kind"] == "match"),
            "{query}: {result:?}"
        );
    }
}

#[test]
fn fuzzy_title_selection_preserves_ids_and_rejects_shared_prefixes() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "guide.adoc",
        "= Guide\n\n[#tls]\n== Explicit target\n\nAnchor content.\n\n== TransportLayerSecurity\n\nFuzzy content.\n\n== Café setup\n\nCoffee content.\n\n== Authorization\n\nAccess control.\n",
    );
    let engine = engine(root.path());
    let exact = content(&engine.read(&read("guide.adoc", "tls"), true).unwrap());
    assert!(exact.contains("Anchor content."), "{exact}");
    assert!(!exact.contains("Fuzzy content."), "{exact}");
    for (selector, expected) in [("TLS", "Fuzzy content."), ("CAFE SET", "Coffee content.")] {
        let result = content(&engine.read(&read("guide.adoc", selector), true).unwrap());
        assert!(result.contains(expected), "{selector}: {result}");
    }
    let result = records(
        &engine
            .read(&read("guide.adoc", "authentication"), true)
            .unwrap(),
    );
    assert!(
        result.iter().any(|item| item["status"] == "not_found"),
        "{result:?}"
    );
}

#[test]
fn fuzzy_search_ranks_tighter_matches_first() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "guide.adoc",
        "= Guide\n\n== a---b---c\n\nGapped.\n\n== abc\n\nContiguous.\n",
    );
    let result = records(&engine(root.path()).find(&find("abc")).unwrap());
    let matches = result
        .iter()
        .filter(|item| item["kind"] == "match")
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 2, "{matches:?}");
    assert_eq!(matches[0]["title"], "abc");
    assert!(matches[0]["score"].as_u64().unwrap() > matches[1]["score"].as_u64().unwrap());
}

#[test]
fn metadata_only_search_matches_return_readable_documents() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "guide.adoc",
        "= Guide\n:keywords: onboarding\n\n== Installation\n\nInstall here.\n",
    );
    let engine = engine(root.path());
    let result = records(&engine.find(&find("onboarding")).unwrap());
    let matches = result
        .iter()
        .filter(|item| item["kind"] == "match")
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "{matches:?}");
    assert_eq!(matches[0]["target_kind"], "document");
    assert_eq!(matches[0]["selector"], "@document");
    let request = read(
        matches[0]["path"].as_str().unwrap(),
        matches[0]["selector"].as_str().unwrap(),
    );
    assert!(content(&engine.read(&request, true).unwrap()).contains("Install here."));
}

#[test]
fn warnings_are_returned_and_strict_processing_rejects_recovery() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "missing.adoc",
        "= Guide\n\n== Installation\n\nUsable content.\n\ninclude::missing.adoc-part[]\n",
    );
    let engine = engine(root.path());
    let mut request = read("missing.adoc", "Installation");
    let result = records(&engine.read(&request, true).unwrap());
    assert!(result.iter().any(|item| item["kind"] == "warning"));
    assert!(result.iter().any(|item| {
        item["text"]
            .as_str()
            .is_some_and(|t| t.contains("Usable content."))
    }));
    request.common.strict = true;
    assert!(engine.read(&request, true).is_err());
}

#[test]
fn metadata_and_references_are_navigable() {
    let root = TempDir::new().unwrap();
    write(root.path(), "guide.adoc", GUIDE);
    let engine = engine(root.path());
    let metadata = records(&engine.metadata(&file("guide.adoc")).unwrap());
    assert!(metadata.iter().any(|item| item["title"] == "Widget Guide"));
    assert!(
        metadata
            .iter()
            .any(|item| item["name"] == "revnumber" && item["value"] == "1.2")
    );
    let links = records(&engine.links(&file("guide.adoc")).unwrap());
    assert!(links.iter().any(|item| item["target"] == "#install"));
    assert!(
        links
            .iter()
            .any(|item| item["target"] == "https://example.com")
    );
    let mut request: ResolveRequest =
        serde_json::from_value(json!({"path":"guide.adoc","target":"#install"})).unwrap();
    assert!(
        records(&engine.resolve(&request).unwrap())
            .iter()
            .any(|item| item["id"] == "install" && item["kind"] == "resolved")
    );
    write(
        root.path(),
        "other.adoc",
        "= Other\n\n[#target]\n== Target\n\nThere.\n",
    );
    request.target = "other.adoc#target".into();
    assert!(
        records(&engine.resolve(&request).unwrap())
            .iter()
            .any(|item| item["id"] == "target")
    );
    request.target = "https://example.com".into();
    assert!(
        records(&engine.resolve(&request).unwrap())
            .iter()
            .any(|item| item["kind"] == "external")
    );
}

#[test]
fn requests_observe_edits_and_cursors_reject_changed_includes() {
    let root = TempDir::new().unwrap();
    write(root.path(), "book.adoc", "= Book\n\ninclude::part.adoc[]\n");
    write(
        root.path(),
        "part.adoc",
        &format!("== Installation\n\n{}\n", "Long paragraph. ".repeat(300)),
    );
    let engine = engine(root.path());
    let mut request = read("book.adoc", "Installation");
    request.common.max_tokens = Some(256);
    let first: Value = serde_json::from_str(&engine.read(&request, true).unwrap()).unwrap();
    request.common.cursor = first["next_cursor"].as_str().map(str::to_owned);
    assert!(request.common.cursor.is_some());
    assert!(engine.read(&request, true).is_ok());
    write(root.path(), "part.adoc", "== Installation\n\nChanged.\n");
    assert!(engine.read(&request, true).is_err());
    request.common.cursor = None;
    request.common.max_tokens = None;
    assert!(content(&engine.read(&request, true).unwrap()).contains("Changed."));
    assert!(!root.path().join(".acdc-mcp").exists());
}

#[test]
fn file_limits_scope_and_cancellation_are_enforced() {
    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    write(outside.path(), "outside.adoc", GUIDE);
    write(root.path(), "large.adoc", &"x".repeat(1024 * 1024 + 1));
    fs::write(root.path().join("binary.adoc"), [b'a', 0, b'b']).unwrap();
    let engine = engine(root.path());
    assert!(engine.tree(&file("large.adoc")).is_err());
    assert!(engine.tree(&file("binary.adoc")).is_err());
    assert!(
        engine
            .tree(&file(outside.path().join("outside.adoc").to_str().unwrap()))
            .is_err()
    );
    let request = find("installation");
    engine.cancelled.cancel();
    assert!(engine.find(&request).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_escape_roots_through_direct_or_include_reads() {
    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    write(
        outside.path(),
        "outside.adoc",
        "== Confidential\n\nPrivate material.\n",
    );
    std::os::unix::fs::symlink(
        outside.path().join("outside.adoc"),
        root.path().join("link.adoc"),
    )
    .unwrap();
    write(
        root.path(),
        "book.adoc",
        "= Book\n\n== Public\n\ninclude::link.adoc[]\n",
    );
    let engine = engine(root.path());
    assert!(engine.tree(&file("link.adoc")).is_err());
    let result = engine.read(&read("book.adoc", "Public"), true).unwrap();
    assert!(!result.contains("Private material."));
    assert!(
        records(&result)
            .iter()
            .any(|item| item["kind"] == "warning")
    );
}

#[test]
fn continuation_retains_the_processing_clock_across_new_engines() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "guide.adoc",
        "= Clock\n\n== Read time\n\n{localdatetime}\n",
    );
    let mut first = engine(root.path());
    first.timestamp = 1_000_000;
    let mut request = file("guide.adoc");
    request.common.max_tokens = Some(256);
    let result: Value = serde_json::from_str(&first.metadata(&request).unwrap()).unwrap();
    request.common.cursor = result["next_cursor"].as_str().map(str::to_owned);
    assert!(request.common.cursor.is_some());
    let mut restarted = engine(root.path());
    restarted.timestamp = 2_000_000;
    assert!(restarted.metadata(&request).is_ok());
    assert_ne!(
        content(&first.read(&read("guide.adoc", "Read time"), true).unwrap()),
        content(
            &restarted
                .read(&read("guide.adoc", "Read time"), true)
                .unwrap()
        )
    );
}

#[test]
fn heading_paths_and_independent_partials_remain_accessible() {
    let root = TempDir::new().unwrap();
    write(root.path(), "guide.adoc", GUIDE);
    write(
        root.path(),
        "partial.adoc",
        "A standalone paragraph with *formatting*.\n",
    );
    let engine = engine(root.path());
    let text = content(
        &engine
            .read(&read("guide.adoc", "installation > LINUX"), true)
            .unwrap(),
    );
    assert!(text.contains("Use the package manager."));
    let code = content(
        &engine
            .read(&read("guide.adoc", "Linux > bootstrap"), false)
            .unwrap(),
    );
    assert!(code.contains("echo '*literal*'"));
    let partial = content(
        &engine
            .read(&read("partial.adoc", "@document"), true)
            .unwrap(),
    );
    assert!(partial.contains("standalone paragraph"));
}

#[test]
fn missing_and_remote_includes_never_hide_available_content() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "guide.adoc",
        "= Guide\n:allow-uri-read:\n\n== Readable\n\nAvailable.\n\ninclude::https://127.0.0.1:1/unreachable.adoc[]\n",
    );
    let engine = engine(root.path());
    let result = engine.read(&read("guide.adoc", "Readable"), true).unwrap();
    assert!(content(&result).contains("Available."));
    assert!(
        records(&result)
            .iter()
            .any(|item| item["kind"] == "warning")
    );
}

#[test]
fn overlapping_roots_do_not_duplicate_search_and_relative_paths_can_be_ambiguous() {
    let parent = TempDir::new().unwrap();
    write(parent.path(), "one/guide.adoc", GUIDE);
    write(parent.path(), "two/guide.adoc", GUIDE);
    let config = configuration(&[parent.path().to_owned(), parent.path().join("one")], &[]);
    let engine = Engine::new(Arc::new(config), CancellationToken::new());
    let result = records(&engine.find(&find("installation")).unwrap());
    assert_eq!(result[0]["documents_scanned"], 2);
    let config = configuration(&[parent.path().join("one"), parent.path().join("two")], &[]);
    let engine = Engine::new(Arc::new(config), CancellationToken::new());
    assert!(
        engine
            .tree(&file("guide.adoc"))
            .unwrap_err()
            .to_string()
            .contains("multiple roots")
    );
}

#[test]
fn references_to_inline_anchors_return_a_readable_owner() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "guide.adoc",
        "= Guide\n\n== Introduction\n\nAn [[inline-anchor]]inline target.\n",
    );
    let engine = engine(root.path());
    let request: ResolveRequest =
        serde_json::from_value(json!({"path":"guide.adoc", "target":"#inline-anchor"})).unwrap();
    let result = records(&engine.resolve(&request).unwrap());
    let resolved = result
        .iter()
        .find(|item| item["kind"] == "resolved")
        .unwrap();
    let selector = resolved["selector"].as_str().unwrap();
    assert_eq!(resolved["target_kind"], "paragraph");
    assert!(
        content(&engine.read(&read("guide.adoc", selector), false).unwrap())
            .contains("inline target")
    );
}

#[test]
fn relative_links_in_includes_resolve_from_their_original_source() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "book.adoc",
        "= Book\n\ninclude::parts/one.adoc[]\n",
    );
    write(
        root.path(),
        "parts/one.adoc",
        "== Start\n\nxref:../other.adoc#end[End]\n",
    );
    write(
        root.path(),
        "other.adoc",
        "= Other\n\n[#end]\n== Destination\n\nEnd here.\n",
    );
    write(root.path(), "other.html", "<html>Generated output</html>");
    let engine = engine(root.path());
    let links = records(&engine.links(&file("book.adoc")).unwrap());
    let link = links.iter().find(|item| item["kind"] == "link").unwrap();
    let mut request: ResolveRequest = serde_json::from_value(
        json!({"path":"book.adoc", "target":link["target"], "source_path":link["source"]["file"]}),
    )
    .unwrap();
    assert!(
        records(&engine.resolve(&request).unwrap())
            .iter()
            .any(|item| item["id"] == "end")
    );
    request.target = "../other.html#end".into();
    let resolved = records(&engine.resolve(&request).unwrap());
    assert!(resolved.iter().any(|item| item["id"] == "end"));
}
