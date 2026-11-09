//! Generate one selected acdc ASG expectation without using converter defaults.

use std::{error::Error, fs, path::Path};

use acdc_parser::{Options, asg, parse_file};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let name = arguments
        .next()
        .ok_or("supply one .adoc filename from fixtures/asg")?;
    let name = Path::new(&name);
    if arguments.next().is_some()
        || name.components().count() != 1
        || name.extension().is_none_or(|ext| ext != "adoc")
    {
        return Err("supply one .adoc filename from fixtures/asg".into());
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/asg")
        .join(name);
    let parsed = parse_file(&source, &Options::default())?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let output = format!("{}\n", serde_json::to_string_pretty(&value)?);
    fs::write(source.with_extension("json"), output)?;
    Ok(())
}
