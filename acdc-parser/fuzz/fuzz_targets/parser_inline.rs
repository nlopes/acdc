#![no_main]

use acdc_parser::{Options, parse_inline};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, ignoring invalid UTF-8
    if let Ok(input) = std::str::from_utf8(data) {
        // Create default parsing options
        let options = Options::builder().build();

        // Attempt to parse inline content
        // This targets the inline preprocessing and parsing logic
        let _ = parse_inline(input, &options);
    }
});
