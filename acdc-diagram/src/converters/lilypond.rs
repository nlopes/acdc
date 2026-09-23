//! `LilyPond` music engraving.

use std::path::PathBuf;

use crate::{
    Format,
    cli::CommandSpec,
    converters::{ConverterOptions, DiagramConverter, Generated, collect_named, option},
    error::Result,
    generate,
    source::{CommandLookup, DiagramSource},
    which::which,
};

/// A paper block that strips `LilyPond`'s default headers and footers, leaving
/// only the engraved music.
const PAPER: &str = "\\paper{\n  oddFooterMarkup=##f\n  oddHeaderMarkup=##f\n  bookTitleMarkup=##f\n  scoreTitleMarkup=##f\n}\n\n";

/// `lilypond`.
pub(crate) struct Lilypond;

impl DiagramConverter for Lilypond {
    fn supported_formats(&self) -> &'static [Format] {
        &[Format::Png, Format::Pdf]
    }

    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        Ok(collect_named(source, &["resolution"]))
    }

    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated> {
        let extra_paths = macos_app_bundle_paths();
        let tool =
            source.find_command(&CommandLookup::new(&["lilypond"]).with_paths(&extra_paths))?;

        let code = format!("{PAPER}{}", source.code());
        let resolution = option(options, "resolution").map(str::to_string);

        generate::stdin_to_file(&tool, format.as_str(), code.as_bytes(), |tool, output| {
            let mut spec = CommandSpec::new(tool)
                .args([
                    "-daux-files=#f",
                    "-dbackend=eps",
                    "-dno-gs-load-fonts",
                    "-dinclude-eps-fonts",
                    "-o",
                ])
                .arg(generate::native(output))
                .args(["-f", format.as_str(), "-dcrop=#t"]);
            if let Some(resolution) = &resolution {
                spec = spec.arg(format!("-dresolution={resolution}"));
            }
            if format == Format::Png {
                spec = spec.arg("-dpixmap-format=pngalpha");
            }
            // LilyPond appends `.cropped.<format>` to the name it is given.
            spec.arg("-")
                .out_file(format!("{}.cropped.{format}", output.display()))
                .chdir(source.base_dir())
        })
        .map(Generated::from)
    }
}

/// On macOS `LilyPond` is commonly installed as an application bundle rather
/// than on `PATH`.
fn macos_app_bundle_paths() -> Vec<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    which("LilyPond.app", &[PathBuf::from("/Applications")])
        .map(|app| vec![app.join("Contents/Resources/bin")])
        .unwrap_or_default()
}
