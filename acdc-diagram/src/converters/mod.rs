//! The diagram generators.
//!
//! Each module here wraps one external tool: it declares the formats the tool
//! can produce, collects the block attributes that change its output, and
//! builds the command line. Everything around that — caching, measuring,
//! rewriting the AST — is shared and lives outside this module.
//!
//! Adding a tool means writing a [`DiagramConverter`] and listing it in
//! [`lookup`] and [`NAMES`].
//!
//! # Not ported
//!
//! `barcode` (a pure-Ruby barcode library) and `structurizr` (the gem's
//! bundled Java rendering server) have no equivalent that can be driven as a
//! subprocess, so acdc leaves both out rather than shipping a half-working
//! version.

mod a2s;
mod blockdiag;
mod bpmn;
mod bytefield;
mod d2;
mod dbml;
mod diagrams;
mod ditaa;
mod dpic;
mod erd;
mod gnuplot;
mod goat;
mod graphviz;
mod java;
mod lilypond;
mod meme;
mod mermaid;
mod msc;
mod nomnoml;
mod oxdraw;
mod penrose;
mod pikchr;
mod pintora;
mod plantuml;
mod shaape;
mod smcat;
mod svgbob;
mod symbolator;
mod syntrax;
mod tikz;
mod umlet;
mod vega;
mod vhs;
mod wavedrom;

use std::collections::BTreeMap;

use crate::{Format, error::Result, source::DiagramSource};

/// The tool settings an image was generated with.
///
/// Stored in the cache sidecar and compared on the next run, so changing
/// `layout=neato` regenerates the image even though the diagram text is
/// unchanged.
pub(crate) type ConverterOptions = BTreeMap<String, String>;

/// What a converter produced.
#[derive(Debug, Default)]
pub(crate) struct Generated {
    /// The image or text itself.
    pub(crate) data: Vec<u8>,
    /// Companion files written next to the image, keyed by the suffix to
    /// append to its name. `PlantUML` uses this for the generated image map.
    pub(crate) extra: Vec<(String, Vec<u8>)>,
}

impl From<Vec<u8>> for Generated {
    fn from(data: Vec<u8>) -> Self {
        Self {
            data,
            extra: Vec::new(),
        }
    }
}

/// One diagram tool.
pub(crate) trait DiagramConverter {
    /// Formats this tool can produce, most preferred first.
    ///
    /// The first entry is the default when the document does not ask for a
    /// particular format.
    fn supported_formats(&self) -> &'static [Format];

    /// Whether the tool applies `scale=` itself.
    ///
    /// When it does not, the scale factor is applied to the measured image
    /// dimensions instead, so the output is still the requested size.
    fn native_scaling(&self) -> bool {
        false
    }

    /// Gather the attributes that change this tool's output.
    ///
    /// # Errors
    ///
    /// Returns an error when an attribute holds a value the tool cannot use.
    fn collect_options(&self, source: &DiagramSource<'_>) -> Result<ConverterOptions> {
        let _ = source;
        Ok(ConverterOptions::new())
    }

    /// Rewrite the diagram code before it is hashed and rendered.
    ///
    /// `PlantUML` uses this to wrap the body in `@startuml`/`@enduml` and to run
    /// the `PlantUML` preprocessor, so that `!include`d files are part of the
    /// checksum.
    ///
    /// # Errors
    ///
    /// Returns an error when the preprocessing step itself fails.
    fn prepare(&self, source: &mut DiagramSource<'_>) -> Result<()> {
        let _ = source;
        Ok(())
    }

    /// Render the diagram.
    ///
    /// # Errors
    ///
    /// Returns an error when the tool is missing, fails, or produces nothing.
    fn convert(
        &self,
        source: &DiagramSource<'_>,
        format: Format,
        options: &ConverterOptions,
    ) -> Result<Generated>;
}

/// Every diagram block style and block macro name acdc recognises.
///
/// Used to decide whether a `[name]` block or a `name::target[]` macro is a
/// diagram at all, before any tool is looked for.
pub(crate) const NAMES: &[&str] = &[
    "a2s",
    "actdiag",
    "blockdiag",
    "bpmn",
    "bytefield",
    "d2",
    "dbml",
    "diagrams",
    "ditaa",
    "dpic",
    "erd",
    "gnuplot",
    "goat",
    "graphviz",
    "graphviz_py",
    "lilypond",
    "meme",
    "mermaid",
    "msc",
    "nomnoml",
    "nwdiag",
    "oxdraw",
    "packetdiag",
    "penrose",
    "pikchr",
    "pintora",
    "plantuml",
    "rackdiag",
    "salt",
    "seqdiag",
    "shaape",
    "smcat",
    "svgbob",
    "symbolator",
    "syntrax",
    "tape",
    "tikz",
    "umlet",
    "vega",
    "vegalite",
    "wavedrom",
];

/// Build the converter registered under `name`, if there is one.
pub(crate) fn lookup(name: &str) -> Option<Box<dyn DiagramConverter>> {
    let converter: Box<dyn DiagramConverter> = match name {
        "a2s" => Box::new(a2s::A2s),
        "actdiag" => Box::new(blockdiag::BlockDiag::new("actdiag")),
        "blockdiag" => Box::new(blockdiag::BlockDiag::new("blockdiag")),
        "nwdiag" => Box::new(blockdiag::BlockDiag::new("nwdiag")),
        "packetdiag" => Box::new(blockdiag::BlockDiag::new("packetdiag")),
        "rackdiag" => Box::new(blockdiag::BlockDiag::new("rackdiag")),
        "seqdiag" => Box::new(blockdiag::BlockDiag::new("seqdiag")),
        "bpmn" => Box::new(bpmn::Bpmn),
        "bytefield" => Box::new(bytefield::Bytefield),
        "d2" => Box::new(d2::D2),
        "dbml" => Box::new(dbml::Dbml),
        "diagrams" => Box::new(diagrams::Diagrams),
        "ditaa" => Box::new(ditaa::Ditaa),
        "dpic" => Box::new(dpic::Dpic),
        "erd" => Box::new(erd::Erd),
        "gnuplot" => Box::new(gnuplot::Gnuplot),
        "goat" => Box::new(goat::Goat),
        "graphviz" => Box::new(graphviz::Graphviz),
        "graphviz_py" => Box::new(graphviz::GraphvizPy),
        "lilypond" => Box::new(lilypond::Lilypond),
        "meme" => Box::new(meme::Meme),
        "mermaid" => Box::new(mermaid::Mermaid),
        "msc" => Box::new(msc::Mscgen),
        "nomnoml" => Box::new(nomnoml::Nomnoml),
        "oxdraw" => Box::new(oxdraw::Oxdraw),
        "penrose" => Box::new(penrose::Penrose),
        "pikchr" => Box::new(pikchr::Pikchr),
        "pintora" => Box::new(pintora::Pintora),
        "plantuml" => Box::new(plantuml::PlantUml::new("uml")),
        "salt" => Box::new(plantuml::PlantUml::new("salt")),
        "shaape" => Box::new(shaape::Shaape),
        "smcat" => Box::new(smcat::Smcat),
        "svgbob" => Box::new(svgbob::Svgbob),
        "symbolator" => Box::new(symbolator::Symbolator),
        "syntrax" => Box::new(syntrax::Syntrax),
        "tape" => Box::new(vhs::Vhs),
        "tikz" => Box::new(tikz::TikZ),
        "umlet" => Box::new(umlet::Umlet),
        "vega" => Box::new(vega::Vega::new(false)),
        "vegalite" => Box::new(vega::Vega::new(true)),
        "wavedrom" => Box::new(wavedrom::Wavedrom),
        _ => return None,
    };
    Some(converter)
}

/// Record `value` under `name` when it is set.
///
/// Options are compared against the cache sidecar, so an unset attribute must
/// leave no entry at all rather than an empty one.
pub(crate) fn set_option(options: &mut ConverterOptions, name: &str, value: Option<String>) {
    if let Some(value) = value {
        options.insert(name.to_string(), value);
    }
}

/// Read an option back out.
pub(crate) fn option<'a>(options: &'a ConverterOptions, name: &str) -> Option<&'a str> {
    options.get(name).map(String::as_str)
}

/// Collect a fixed list of attributes whose names match their option names.
///
/// Most tools expose their knobs as `foo-bar=` attributes that map onto a
/// `--foo-bar` flag; this covers that case in one line.
pub(crate) fn collect_named(source: &DiagramSource<'_>, names: &[&str]) -> ConverterOptions {
    let mut options = ConverterOptions::new();
    for name in names {
        set_option(&mut options, name, source.attr(&[name]));
    }
    options
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn every_registered_name_resolves() {
        for name in NAMES {
            assert!(
                lookup(name).is_some(),
                "`{name}` is listed in NAMES but has no converter"
            );
        }
    }

    #[test]
    fn unknown_names_do_not_resolve() {
        assert!(lookup("source").is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn every_converter_offers_a_default_format() {
        for name in NAMES {
            let converter = lookup(name).expect("registered");
            assert!(
                !converter.supported_formats().is_empty(),
                "`{name}` declares no output formats"
            );
        }
    }
}
