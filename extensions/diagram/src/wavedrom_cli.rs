//! `WaveDrom` file-to-file adapter ported from the acdc-diagram prototype (PR #451).

use std::{fs, io, path::Path, process::Command};

pub(super) fn render(tool: &Path, body: &str, scratch: &Path, output: &Path) -> io::Result<()> {
    let input = scratch.join("input.wvd");
    fs::write(&input, body)?;
    super::asset::run(
        Command::new(tool)
            .arg("--input")
            .arg(input)
            .arg("--svg")
            .arg(output),
        scratch,
        "renderer",
    )
}
