#![cfg(feature = "images")]

use std::{env, process::Command};

use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_converters_terminal::{Capabilities, Processor};
use acdc_parser::{Options as ParserOptions, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn index_section_does_not_display_images_twice() -> Result<(), Error> {
    let executable = env::current_exe()?;
    let image_blocks = |mode| -> Result<usize, Error> {
        // viuer writes outside the converter's writer; capture the process output.
        let output = Command::new(&executable)
            .args(["--exact", "render_index_image_probe", "--nocapture"])
            .env("ACDC_INDEX_IMAGE_PROBE", mode)
            .env("TERM", "dumb")
            .env("COLORTERM", "truecolor")
            .env_remove("TERM_PROGRAM")
            .env_remove("KITTY_WINDOW_ID")
            .env_remove("ITERM_SESSION_ID")
            .output()?;
        assert!(output.status.success(), "{output:?}");
        Ok(String::from_utf8(output.stdout)?.matches('▄').count())
    };
    let control = image_blocks("control")?;
    assert!(control > 0, "the control must display an image");
    assert_eq!(image_blocks("index")?, control);
    Ok(())
}

#[test]
fn render_index_image_probe() -> Result<(), Error> {
    let Ok(mode) = env::var("ACDC_INDEX_IMAGE_PROBE") else {
        return Ok(());
    };
    let image = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../acdc-parser/fixtures/samples/book-starter/images/cover.jpeg"
    );
    let mut input = format!("= Image\n\nimage::{image}[Cover,2,2]\n");
    if mode == "index" {
        input.push_str("\n[index]\n== Index\n");
    }
    let parsed = parse(&input, &ParserOptions::default())?;
    let processor = Processor::new(Options::default(), ParserOptions::builder())?
        .with_terminal_width(80)
        .with_terminal_capabilities(Capabilities {
            unicode: true,
            osc8_links: false,
        });
    let source = WarningSource::new("terminal");
    let mut warnings = Vec::new();
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        parsed.document(),
        &mut Vec::new(),
        None,
        None,
        &mut diagnostics,
    )?;
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(())
}
