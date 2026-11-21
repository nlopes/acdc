use acdc_converters_core::pipeline::{ExtensionCatalog, Extensions};
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
        } else if args.diagram.wavedrom_cli.is_some()
            || args.diagram.lualatex.is_some()
            || args.diagram.graphviz_dot.is_some()
        {
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
    /// Path to the trusted Graphviz dot executable
    #[arg(long, value_name = "PATH")]
    graphviz_dot: Option<std::path::PathBuf>,
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
    fn select(
        &self,
        args: &Args,
        catalog: &ExtensionCatalog,
        asset_base: AssetBase,
    ) -> Result<Selection> {
        use std::collections::{HashMap, hash_map::Entry};
        if self.wavedrom_cli.is_none() && self.lualatex.is_none() && self.graphviz_dot.is_none() {
            return Err(miette::miette!(
                "--extension diagram requires --wavedrom-cli PATH, --lualatex PATH, or --graphviz-dot PATH"
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
        use acdc_parser::extensions::Registry;
        let output = acdc_diagram::OutputDirectories::new(linked, embedded)
            .with_imagesoutdir_base(Path::new("."));
        let mut registry = Registry::builder();
        if let Some(tool) = &self.graphviz_dot {
            let style = acdc_diagram::graphviz(tool, output).map_err(|e| {
                miette::miette!("cannot configure Graphviz with {}: {e}", tool.display())
            })?;
            registry
                .register_block_style(style)
                .map_err(|e| error::display(&e))?;
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
        let mut catalog = available.clone();
        catalog
            .register("diagram", registry.build())
            .map_err(|e| error::display(&e))?;
        catalog
            .select(&args.extensions)
            .map_err(|e| error::display(&e))
    }
}
