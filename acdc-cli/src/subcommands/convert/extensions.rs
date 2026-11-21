use acdc_converters_core::pipeline::{ExtensionCatalog, Extensions};
#[cfg(feature = "diagram")]
use acdc_diagram::OutputDirectories;
#[cfg(feature = "diagram")]
use acdc_parser::extensions::{BlockMacro, BlockStyle, Registry, RegistryBuilder};
use miette::Result;
use std::path::Path;

use super::Args;
use crate::error;

/// Directory the selected converter uses to resolve linked images.
#[derive(Clone, Copy)]
pub(super) enum AssetBase {
    Output,
    #[cfg(feature = "pdf")]
    Source,
    #[cfg(feature = "terminal")]
    WorkingDirectory,
}

#[derive(Debug)]
pub(super) enum Selection {
    Shared(Option<Extensions>),
    #[cfg(feature = "diagram")]
    ByDirectory(std::collections::HashMap<std::path::PathBuf, Option<Extensions>>),
}

impl Selection {
    pub(super) fn new(
        args: &Args,
        catalog: &ExtensionCatalog,
        asset_base: AssetBase,
    ) -> Result<Self> {
        #[cfg(feature = "diagram")]
        if args.extensions.iter().any(|name| name == "diagram") {
            return args.diagram.select(args, catalog, asset_base);
        } else if args.diagram.has_renderer() {
            return Err(miette::miette!(
                "diagram tool paths require --extension diagram"
            ));
        }
        #[cfg(not(feature = "diagram"))]
        let _ = asset_base;
        catalog
            .select(&args.extensions)
            .map(Self::Shared)
            .map_err(|e| error::display(&e))
    }

    pub(super) fn for_input(&self, input: Option<&Path>) -> Option<&Extensions> {
        #[cfg(not(feature = "diagram"))]
        let _ = input;
        match self {
            Self::Shared(selected) => selected.as_ref(),
            #[cfg(feature = "diagram")]
            Self::ByDirectory(dirs) => dirs.get(parent(input)).and_then(Option::as_ref),
        }
    }
}

#[cfg(feature = "diagram")]
fn parent(file: Option<&Path>) -> &Path {
    file.and_then(Path::parent)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(feature = "diagram")]
#[derive(clap::Args, Debug)]
pub(super) struct DiagramArgs {
    /// Path to the trusted blockdiag renderer
    #[arg(long, value_name = "PATH")]
    blockdiag: Option<std::path::PathBuf>,
    /// Path to the trusted seqdiag renderer
    #[arg(long, value_name = "PATH")]
    seqdiag: Option<std::path::PathBuf>,
    /// Path to the trusted actdiag renderer
    #[arg(long, value_name = "PATH")]
    actdiag: Option<std::path::PathBuf>,
    /// Path to the trusted bytefield-svg executable
    #[arg(long, value_name = "PATH")]
    bytefield_svg: Option<std::path::PathBuf>,
    /// Path to the trusted `GoAT` ASCII diagram renderer
    #[arg(long, value_name = "PATH")]
    goat: Option<std::path::PathBuf>,
    /// Path to the trusted Pikchr executable
    #[arg(long, value_name = "PATH")]
    pikchr: Option<std::path::PathBuf>,
    /// Path to the trusted dpic executable for PIC diagrams
    #[arg(long, value_name = "PATH")]
    dpic: Option<std::path::PathBuf>,
    /// Path to the trusted Graphviz dot executable
    #[arg(long, value_name = "PATH")]
    graphviz_dot: Option<std::path::PathBuf>,
    /// Path to the trusted mscgen executable for message sequence charts
    #[arg(long, value_name = "PATH")]
    mscgen: Option<std::path::PathBuf>,
    /// Path to the trusted wavedrom-cli executable
    #[arg(long, value_name = "PATH")]
    wavedrom_cli: Option<std::path::PathBuf>,
    /// Path to the trusted lualatex executable
    #[arg(long, value_name = "PATH")]
    lualatex: Option<std::path::PathBuf>,
    /// Path to the trusted pdf2svg executable for SVG output
    #[arg(long, value_name = "PATH", requires = "lualatex")]
    pdf2svg: Option<std::path::PathBuf>,
}

#[cfg(feature = "diagram")]
impl DiagramArgs {
    fn configured_renderers(
        &self,
    ) -> impl Iterator<Item = (acdc_diagram::renderer::Renderer, &Path)> {
        [
            (acdc_diagram::renderer::ACTDIAG, self.actdiag.as_deref()),
            (acdc_diagram::renderer::SEQDIAG, self.seqdiag.as_deref()),
            (acdc_diagram::renderer::BLOCKDIAG, self.blockdiag.as_deref()),
        ]
        .into_iter()
        .filter_map(|(renderer, path): (_, Option<&Path>)| path.map(|p| (renderer, p)))
    }

    fn has_renderer(&self) -> bool {
        self.wavedrom_cli.is_some()
            || self.lualatex.is_some()
            || self.graphviz_dot.is_some()
            || self.mscgen.is_some()
            || self.dpic.is_some()
            || self.pikchr.is_some()
            || self.goat.is_some()
            || self.bytefield_svg.is_some()
            || self.configured_renderers().next().is_some()
    }

    fn select(
        &self,
        args: &Args,
        catalog: &ExtensionCatalog,
        asset_base: AssetBase,
    ) -> Result<Selection> {
        use std::collections::{HashMap, hash_map::Entry};
        if !self.has_renderer() {
            return Err(miette::miette!(
                "--extension diagram requires --wavedrom-cli PATH, --lualatex PATH, --graphviz-dot PATH, --mscgen PATH, --dpic PATH, --pikchr PATH, --goat PATH, or --bytefield-svg PATH"
            ));
        }
        // The terminal backend displays files from the working directory, including in a pager.
        #[cfg(feature = "terminal")]
        if matches!(asset_base, AssetBase::WorkingDirectory) {
            let cwd = Path::new(".");
            return self
                .register(args, catalog, cwd, cwd)
                .map(Selection::Shared);
        }

        let select = |source: &Path| {
            let linked = match args.out_file.as_deref() {
                Some("-") => Path::new("."),
                Some(file) => parent(Some(Path::new(file))),
                None => source,
            };
            // PDF reads ordinary image nodes relative to the source, even with -o.
            #[cfg(feature = "pdf")]
            let linked = if matches!(asset_base, AssetBase::Source) {
                source
            } else {
                linked
            };
            #[cfg(not(any(feature = "terminal", feature = "pdf")))]
            let _ = asset_base;
            self.register(args, catalog, linked, source)
        };

        if args.out_file.is_some() || args.files.len() <= 1 {
            return select(parent(args.files.first().map(std::path::PathBuf::as_path)))
                .map(Selection::Shared);
        }
        let mut dirs = HashMap::new();
        for file in &args.files {
            let dir = parent(Some(file));
            if let Entry::Vacant(entry) = dirs.entry(dir.to_owned()) {
                entry.insert(select(dir)?);
            }
        }
        Ok(Selection::ByDirectory(dirs))
    }

    fn register(
        &self,
        args: &Args,
        available: &ExtensionCatalog,
        linked: &Path,
        embedded: &Path,
    ) -> Result<Option<Extensions>> {
        let output =
            OutputDirectories::new(linked, embedded).with_imagesoutdir_base(Path::new("."));
        let mut registry = Registry::builder();
        if let Some(tool) = &self.graphviz_dot {
            register_file_renderer(
                &mut registry,
                "Graphviz",
                tool,
                output,
                acdc_diagram::graphviz,
                acdc_diagram::graphviz_file,
            )?;
        }
        if let Some(tool) = &self.mscgen {
            register_file_renderer(
                &mut registry,
                "mscgen",
                tool,
                output,
                acdc_diagram::msc,
                acdc_diagram::msc_file,
            )?;
        }
        if let Some(tool) = &self.dpic {
            register_file_renderer(
                &mut registry,
                "dpic",
                tool,
                output,
                acdc_diagram::dpic,
                acdc_diagram::dpic_file,
            )?;
        }
        if let Some(tool) = &self.pikchr {
            register_file_renderer(
                &mut registry,
                "pikchr",
                tool,
                output,
                acdc_diagram::pikchr,
                acdc_diagram::pikchr_file,
            )?;
        }
        if let Some(tool) = &self.goat {
            register_file_renderer(
                &mut registry,
                "goat",
                tool,
                output,
                acdc_diagram::goat,
                acdc_diagram::goat_file,
            )?;
        }
        if let Some(tool) = &self.bytefield_svg {
            register_file_renderer(
                &mut registry,
                "bytefield-svg",
                tool,
                output,
                acdc_diagram::bytefield,
                acdc_diagram::bytefield_file,
            )?;
        }
        if let Some(tool) = &self.wavedrom_cli {
            let style = acdc_diagram::wavedrom(tool, output).map_err(|e| {
                miette::miette!("cannot configure WaveDrom with {}: {e}", tool.display())
            })?;
            registry
                .register_block_style(style)
                .map_err(|e| error::display(&e))?;
        }
        if let Some(tool) = &self.lualatex {
            let style = acdc_diagram::tikz(tool, self.pdf2svg.as_deref(), output).map_err(|e| {
                miette::miette!("cannot configure TikZ with {}: {e}", tool.display())
            })?;
            registry
                .register_block_style(style)
                .map_err(|e| error::display(&e))?;
        }
        for (renderer, tool) in self.configured_renderers() {
            let (style, file) = renderer
                .processors(tool, output)
                .map_err(|e| miette::miette!("cannot configure {}: {e}", tool.display()))?;
            registry
                .register_block_style(style)
                .map_err(|e| error::display(&e))?;
            registry.register(file).map_err(|e| error::display(&e))?;
        }
        let mut catalog = available.clone();
        catalog
            .register("diagram", registry.build())
            .map_err(|e| error::display(&e))?;
        catalog
            .select(&args.extensions)
            .map_err(|e| error::display(&e))
    }
}

#[cfg(feature = "diagram")]
fn register_file_renderer<'a>(
    registry: &mut RegistryBuilder,
    name: &str,
    tool: &Path,
    output: OutputDirectories<'a>,
    style: impl FnOnce(&Path, OutputDirectories<'a>) -> std::io::Result<BlockStyle>,
    file_macro: impl FnOnce(&Path, OutputDirectories<'a>) -> std::io::Result<BlockMacro>,
) -> Result<()> {
    let configure =
        |e: std::io::Error| miette::miette!("cannot configure {name} with {}: {e}", tool.display());
    let style = style(tool, output).map_err(configure)?;
    let file_macro = file_macro(tool, output).map_err(configure)?;
    registry
        .register_block_style(style)
        .map_err(|e| error::display(&e))?;
    registry
        .register(file_macro)
        .map_err(|e| error::display(&e))?;
    Ok(())
}
