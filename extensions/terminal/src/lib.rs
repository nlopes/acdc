//! Native terminal previews and replay, explicitly selected before conversion.

mod render;
mod renderer;

use acdc_converters_core::{pipeline::Extensions, render::BlockRenderer};
use acdc_parser::ast::AttributeValueRef;

pub use render::{REPLAY_PLAYER_SCRIPT, REPLAY_PLAYER_SCRIPT_CSP_HASH};

/// Terminal rendering failures retain the emulator, replay, or output error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Terminal(#[from] acdc_converters_terminal::Error),
    #[error(transparent)]
    Grid(#[from] acdc_converters_terminal::cell_grid::Error),
    #[error(transparent)]
    Replay(#[from] acdc_converters_terminal::replay::Error),
}

/// Make terminal rendering available without constructing a processor.
/// Register this package as `terminal` in an `ExtensionCatalog`, then select it
/// explicitly, or attach it directly with `Pipeline::with_extensions`.
#[must_use]
pub fn extension() -> Extensions {
    Extensions::default().with_renderer(
        BlockRenderer::new("html", || renderer::Terminal)
            .with_html_script_hashes(&[REPLAY_PLAYER_SCRIPT_CSP_HASH]),
    )
}

fn attribute_scalar(value: AttributeValueRef<'_>) -> Option<&str> {
    match value {
        AttributeValueRef::Text(value) => Some(value),
        AttributeValueRef::Bool(value) => Some(if value { "true" } else { "false" }),
        AttributeValueRef::None => Some("null"),
        AttributeValueRef::Content(_)
        | AttributeValueRef::ContentList(_)
        | AttributeValueRef::Tokens(_)
        | AttributeValueRef::Substitutions(_)
        | _ => None,
    }
}
