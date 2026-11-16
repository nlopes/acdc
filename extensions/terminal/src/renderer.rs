use acdc_converters_core::render::{BlockRenderProcessor, RenderError, RenderHost};
use acdc_parser::{
    SafeMode,
    ast::{BlockKind, BlockRef},
};

use crate::render::{is_terminal_listing, is_terminal_session, render_listing, render_session};

pub(super) struct Terminal;

impl BlockRenderProcessor for Terminal {
    fn script_hashes(&self) -> &'static [&'static str] {
        &[crate::REPLAY_PLAYER_SCRIPT_CSP_HASH]
    }

    fn render(
        &mut self,
        block: BlockRef<'_>,
        host: &mut dyn RenderHost,
    ) -> Result<bool, RenderError> {
        let (content, attributes, session) = if let BlockKind::Listing(data) = block.kind() {
            let session = is_terminal_session(data.attributes());
            if !session && !is_terminal_listing(host.document_attributes(), data.attributes()) {
                return Ok(false);
            }
            (data.content(), data.attributes(), session)
        } else if let BlockKind::Literal(data) = block.kind()
            && is_terminal_session(data.attributes())
        {
            (data.content(), data.attributes(), true)
        } else {
            return Ok(false);
        };
        let options = host.options().clone();
        let safe_mode = options.safe_mode();
        if safe_mode >= SafeMode::Server {
            host.output(&mut |_, diagnostics| {
                diagnostics.warn_with_advice(
                    format!("terminal rendering is disabled at safe mode `{safe_mode:?}`; rendering the block as a plain listing"),
                    "Use terminal previews only for trusted documents below Server safe mode, or use a plain listing block.",
                );
                Ok(())
            })?;
            return Ok(false);
        }
        let attrs = host.document_attributes().clone().into_static();
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
        host.output(&mut |writer, diagnostics| {
            if session {
                render_session(
                    writer,
                    content,
                    attributes,
                    options.clone(),
                    &attrs,
                    diagnostics,
                )?;
            } else {
                render_listing(
                    writer,
                    content,
                    attributes,
                    options.clone(),
                    &attrs,
                    diagnostics,
                )?;
            }
            Ok(())
        })?;
        if !semantic {
            host.write(b"</div>\n")?;
        }
        host.write(format!("</{tag}>\n").as_bytes())?;
        Ok(true)
    }
}
