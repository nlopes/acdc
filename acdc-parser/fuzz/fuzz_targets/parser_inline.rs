#![no_main]

use libfuzzer_sys::fuzz_target;
use acdc_parser::{parse_inline, DocumentAttributes, Options};

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, ignoring invalid UTF-8
    if let Ok(input) = std::str::from_utf8(data) {
        // Create default parsing options
        let options = Options {
            document_attributes: DocumentAttributes::default(),
            ..Default::default()
        };

        // Attempt to parse inline content
        // This targets the inline preprocessing and parsing logic
        let _ = parse_inline(input, &options);
    }
});
