//! `WaveDrom` file-to-file adapter ported from the acdc-diagram prototype (PR #451).

use std::{
    fs,
    io::{self, Read as _},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use wait_timeout::ChildExt as _;

const MAX_SVG_BYTES: u64 = 16 * 1024 * 1024;

// Use the same --input/--svg protocol as the prototype. Keeping scratch
// files beside the destination makes the final rename atomic.
pub(super) fn render(tool: &Path, body: &str, dir: &Path, filename: &str) -> io::Result<()> {
    let scratch = tempfile::Builder::new()
        .prefix(".acdc-wavedrom-")
        .tempdir_in(dir)?;
    let input = scratch.path().join("input.wvd");
    let output = scratch.path().join("output.svg");
    fs::write(&input, body)?;
    let mut child = Command::new(tool)
        .arg("--input")
        .arg(&input)
        .arg("--svg")
        .arg(&output)
        .current_dir(scratch.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let Some(status) = child.wait_timeout(Duration::from_secs(30))? else {
        child.kill()?;
        child.wait()?;
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "renderer exceeded 30 seconds",
        ));
    };
    if !status.success() {
        return Err(io::Error::other(format!("renderer exited with {status}")));
    }
    let mut svg = String::new();
    fs::File::open(&output)?
        .take(MAX_SVG_BYTES + 1)
        .read_to_string(&mut svg)?;
    if svg.len() as u64 > MAX_SVG_BYTES {
        return Err(io::Error::other("SVG exceeds the 16 MiB limit"));
    }
    let document = roxmltree::Document::parse(&svg).map_err(io::Error::other)?;
    if !document
        .root_element()
        .has_tag_name(("http://www.w3.org/2000/svg", "svg"))
    {
        return Err(io::Error::other("renderer output has no SVG root element"));
    }
    fs::rename(output, dir.join(filename))
}
