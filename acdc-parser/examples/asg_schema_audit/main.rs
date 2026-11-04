//! Audit native fixture shapes against the pinned ASG schema without rewriting them.

#[allow(dead_code)]
#[path = "../../tests/support/fixtures.rs"]
mod fixtures;
mod model;
mod schema;

use std::{fs, path::PathBuf, process::ExitCode};

use serde_json::{Value, json};

fn run() -> schema::Result<bool> {
    let mut args = std::env::args_os().skip(1);
    let selection = args.next();
    if selection
        .as_deref()
        .is_some_and(|arg| arg == "--model-shape")
    {
        if args.next().is_some() {
            return Err("unexpected argument".into());
        }
        println!("{}", serde_json::to_string_pretty(&model::collect()?)?);
        return Ok(true);
    }
    let (_, validator, locations) = schema::load()?;
    let controls = schema::controls(&validator)?;
    let mut files = Vec::new();
    let mut fixture_root = None;
    let validating = selection.as_deref().is_some_and(|arg| arg == "--validate");
    if validating {
        files.extend(args.map(PathBuf::from));
        if files.is_empty() {
            return Err("usage: asg_schema_audit --validate FILE...".into());
        }
    } else {
        let root = match selection.as_deref() {
            None => fixtures::fixture_root(),
            Some(arg) if arg == "--fixtures" => {
                let root = args.next().ok_or("--fixtures requires a directory")?;
                if args.next().is_some() {
                    return Err("unexpected argument".into());
                }
                PathBuf::from(root)
            }
            _ => {
                return Err("usage: asg_schema_audit [--fixtures DIR | --validate FILE...]".into());
            }
        };
        fixture_root = Some(root.clone());
        for source in fixtures::discover_sources(&root)? {
            fixtures::Case::load(&root, &source, false)?;
            for suffix in ["json", "asciidoctor.json", "draft.json"] {
                let path = source.with_extension(suffix);
                if path.is_file() {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    let (mut valid, mut invalid, mut malformed) = (0, 0, 0);
    let mut reports = Vec::new();
    for path in &files {
        let display = fixture_root
            .as_ref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path);
        let value: Value = match serde_json::from_slice(&fs::read(path)?) {
            Ok(value) => value,
            Err(error) => {
                malformed += 1;
                reports.push(json!({"file":display,"invalid_json":error.to_string()}));
                continue;
            }
        };
        let errors = schema::failures(&validator, &value);
        let accepted = errors.is_empty();
        if accepted {
            valid += 1;
        } else {
            invalid += 1;
        }
        let mut findings = Vec::new();
        schema::locations(&value, "", &locations, &mut findings);
        reports.push(json!({"file":display,"schema_valid":accepted,
            "schema_errors":errors,"location_findings":findings}));
    }
    let report = json!({"schema":{"revision":schema::REVISION,"id":schema::ID,
        "sha256":schema::SHA256},"controls":controls,
        "scope":if validating {"Supplied JSON validation; no export or language-conformance claim."}
            else {"Native fixture shape audit; schema failures do not establish parser or exporter defects."},
        "counts":{"files":files.len(),"schema_valid":valid,"schema_invalid":invalid,"invalid_json":malformed},
        "files":reports});
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(!validating || (invalid == 0 && malformed == 0))
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("ASG schema audit failed: {error}");
            ExitCode::FAILURE
        }
    }
}
