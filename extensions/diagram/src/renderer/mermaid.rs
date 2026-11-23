use super::Job;
use crate::asset;
use std::{io, process::Command};
pub(super) fn render(job: &Job<'_>) -> io::Result<()> {
    let input = job.source_file("mmd")?;
    let mut cmd = Command::new(job.tool);
    cmd.arg("-i").arg(input).arg("-o").arg(job.output);
    if let Some(scale) = job.option("scale")
        && (scale.is_empty() || !scale.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(io::Error::other(
            "mermaid scale must be an unsigned integer",
        ));
    }
    job.flags(
        &mut cmd,
        &[
            ("css", "--cssFile"),
            ("theme", "--theme"),
            ("width", "--width"),
            ("height", "--height"),
            ("scale", "--scale"),
        ],
    );
    if let Some(background) = job.option("background") {
        cmd.arg("--backgroundColor")
            .arg(format!("#{}", background.trim_start_matches('#')));
    }
    if let Some(config) = job.option("config") {
        cmd.arg("--configFile").arg(config);
    } else {
        let mut sections = Vec::new();
        for (key, alias, name) in [
            ("ganttconfig", "gantt-config", "gantt"),
            ("sequenceconfig", "sequence-config", "sequence"),
        ] {
            if let Some(value) = job.option(key).or_else(|| job.option(alias)) {
                let fragment = asset::read_text(&job.dir.join(value))?;
                sections.push(format!(r#""{name}": {fragment}"#));
            }
        }
        if !sections.is_empty() {
            let path = job.scratch.join("config.json");
            std::fs::write(&path, format!("{{{}}}", sections.join(",")))?;
            cmd.arg("--configFile").arg(path);
        }
    }
    if let Some(path) = job
        .option("puppeteerconfig")
        .or_else(|| job.option("puppeteer-config"))
    {
        cmd.arg("--puppeteerConfigFile").arg(path);
    }
    job.run(cmd.env("NODE_OPTIONS", "--unhandled-rejections=strict"))
}
