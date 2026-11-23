use super::Job;
use crate::asset::{self, Format};
use std::{io, process::Command};
pub(super) fn render(job: &Job<'_>) -> io::Result<()> {
    let mut cmd = Command::new(job.tool);
    job.flags(
        &mut cmd,
        &[("background", "--background"), ("scale", "--scale")],
    );
    job.stdin(
        cmd.args(["--input", "-", "--output"])
            .arg(job.output)
            .args(["--output-format", job.format.extension(), "--quiet"]),
        None,
    )?;
    if job.format == Format::Svg
        && let Some(scale) = job.option("scale")
    {
        let scale = scale.parse::<f64>().map_err(io::Error::other)?;
        if !scale.is_finite() || scale < 0.0 {
            return Err(io::Error::other("scale must be finite and nonnegative"));
        }
        let mut svg = asset::read_text(job.output)?;
        let edits = {
            let doc = roxmltree::Document::parse(&svg).map_err(io::Error::other)?;
            let root = doc.root_element();
            root.attributes()
                .filter(|a| matches!(a.name(), "width" | "height"))
                .filter_map(|a| {
                    let value = a.value().parse::<f64>().ok()?;
                    Some((a.range_value(), (value * scale).trunc().to_string()))
                })
                .collect::<Vec<_>>()
        };
        for (range, value) in edits.into_iter().rev() {
            svg.replace_range(range, &value);
        }
        std::fs::write(job.output, svg)?;
    }
    Ok(())
}
