//! Converter callbacks over typed AST reads.
//!
//! Use `DocumentVisitor` for document framing, `BlockVisitor` for block dispatch,
//! and `InlineVisitor` for inline content. Callbacks select their children;
//! nested blocks pass through `TraversalContext` to replay document attributes.

use acdc_parser::ast::{InlineKind, InlineRef};

mod block;
mod document;
mod inline;

pub use block::BlockVisitor;
pub use document::DocumentVisitor;
pub use inline::InlineVisitor;

/// Returns true if the inline node is a formatting span whose children
/// should suppress em-dash boundary replacement at string start/end.
#[must_use]
pub fn is_formatting_span<'inline>(node: impl Into<InlineRef<'inline>>) -> bool {
    matches!(
        node.into().kind(),
        InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Superscript(_)
            | InlineKind::Subscript(_)
            | InlineKind::DoubleQuotedSpan(_)
            | InlineKind::SingleQuotedSpan(_)
    )
}
