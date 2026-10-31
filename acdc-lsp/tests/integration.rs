//! End-to-end language-server integration tests.

mod common;
mod integration {
    mod authored_ids;
    mod backend_options;
    mod completion_state;
    mod coordinate_audit;
    mod coordinates;
    mod hover_content;
    mod includes;
    mod inlay_hints;
    mod link_navigation;
    mod preview;
    mod selection_range;
    mod source_edits;
    mod symbol_text;
    mod xref_content;
}
