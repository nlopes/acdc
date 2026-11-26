use acdc_converters_core::render::{BlockRenderProcessor, Next, RenderContext, RenderError};
use acdc_parser::{
    SafeMode,
    ast::{BlockKind, BlockRef},
};

use crate::render::{is_terminal_listing, is_terminal_session, render_listing, render_session};

pub(super) struct Terminal;

impl BlockRenderProcessor for Terminal {
    fn accepts(
        &mut self,
        block: BlockRef<'_>,
        context: &mut dyn RenderContext,
    ) -> Result<bool, RenderError> {
        let matches = if let BlockKind::Listing(data) = block.kind() {
            is_terminal_session(data.attributes())
                || is_terminal_listing(context.document_attributes(), data.attributes())
        } else {
            matches!(block.kind(),BlockKind::Literal(data) if is_terminal_session(data.attributes()))
        };
        let safe_mode = context.options().safe_mode();
        if matches && safe_mode >= SafeMode::Server {
            context.diagnostics().warn_with_advice(
                    format!("terminal rendering is disabled at safe mode `{safe_mode:?}`; rendering the block as a plain listing"),
                    "Use terminal previews only for trusted documents below Server safe mode, or use a plain listing block.",
                );
            return Ok(false);
        }
        Ok(matches)
    }
    fn render(&mut self, block: BlockRef<'_>, mut next: Next<'_, '_>) -> Result<(), RenderError> {
        let (content, attributes) = if let BlockKind::Listing(data) = block.kind() {
            (data.content(), data.attributes())
        } else if let BlockKind::Literal(data) = block.kind() {
            (data.content(), data.attributes())
        } else {
            return Err("unsupported terminal block".into());
        };
        let session = is_terminal_session(attributes);
        let host = next.host();
        let semantic = host.variant() == "html5s";
        let titled = block.title().is_some();
        let tag = if semantic && titled { "figure" } else { "div" };
        let classes = match (semantic, session) {
            (false, true) => "terminalblock terminal-block",
            (false, false) => "listingblock terminal-block",
            (true, true) => "terminal-block",
            (true, false) => "listing-block terminal-block",
        };
        host.open_block(block, tag, classes)?;
        if semantic {
            host.title(block, "<figcaption>", "</figcaption>\n")?;
        } else {
            host.title(block, "<div class=\"title\">", "</div>\n")?;
            host.write(b"<div class=\"content\">\n")?;
        }
        let (writer, mut diagnostics, attrs) = host.output();
        if session {
            render_session(writer, content, attributes, attrs, &mut diagnostics)?;
        } else {
            render_listing(writer, content, attributes, attrs, &mut diagnostics)?;
        }
        if !semantic {
            host.write(b"</div>\n")?;
        }
        host.write(format!("</{tag}>\n").as_bytes())?;
        Ok(())
    }
}
