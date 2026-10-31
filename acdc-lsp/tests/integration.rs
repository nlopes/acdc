//! End-to-end language-server integration tests.

mod common;
mod integration {
    mod authored_ids;
    mod backend_options;
    mod completion_state;
    mod coordinate_audit;
    mod coordinates;
    mod includes;
    mod inlay_hints;
    mod preview;
    mod selection_range;
    mod source_edits;
    mod symbol_text;
    mod xref_content;
}
