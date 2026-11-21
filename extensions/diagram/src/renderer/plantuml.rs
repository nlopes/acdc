use super::Job;
use crate::asset;
use crate::asset::Format;
use std::{io, process::Command};
pub(super) fn render(job: &Job<'_>) -> io::Result<()> {
    let tag = if job.language == "salt" {
        "salt"
    } else {
        "uml"
    };
    let wrapped;
    let source = if job.source.contains("@start") && job.source.contains("@end") {
        job.source
    } else {
        wrapped = format!("@start{tag}\n{}\n@end{tag}", job.source);
        &wrapped
    };
    let command = || {
        let mut cmd = Command::new(job.tool);
        cmd.args([
            "-pipe",
            "-charset",
            "UTF-8",
            "-failfast2",
            "-stdrpt:1",
            "-filedir",
        ])
        .arg(job.dir);
        if let Some(config) = job.option("config") {
            cmd.arg("-config").arg(job.dir.join(config));
        }
        if let Some(dir) = job.option("includedir") {
            let mut arg = std::ffi::OsString::from("-Dplantuml.include.path=");
            arg.push(job.dir.join(dir));
            cmd.arg(arg);
        }
        job.flags(&mut cmd, &[("theme", "-theme")]);
        cmd.env(
            "PLANTUML_LIMIT_SIZE",
            job.option("size-limit").unwrap_or("4096"),
        );
        cmd
    };
    let expanded;
    let code = if job.option("preprocess") != Some("false") && source.contains(['!', '$', '%']) {
        let output = job.scratch.join("expanded.puml");
        asset::run_with_source(
            command().arg("-preproc"),
            job.dir,
            job.name,
            job.scratch,
            source,
            Some(&output),
        )?;
        expanded = asset::read_text(&output)?;
        &expanded
    } else {
        source
    };
    let mut cmd = command();
    cmd.arg(match job.format {
        Format::Svg => "-tsvg",
        Format::Png => "-tpng",
        Format::Atxt => "-ttxt",
        Format::Txt | Format::Utxt => "-tutxt",
        Format::Gif | Format::Jpeg | Format::Pdf => {
            return Err(io::Error::other("unsupported PlantUML format"));
        }
    });
    if let Ok(dot) = job.companion("dot")
        && job.option("smetana").is_none()
    {
        cmd.arg("-graphvizdot").arg(dot);
    } else {
        cmd.arg("-Playout=smetana");
    }
    if job.option("debug").is_some() {
        cmd.arg("-v");
    }
    asset::run_with_source(
        &mut cmd,
        job.dir,
        job.name,
        job.scratch,
        code,
        Some(job.output),
    )
}
