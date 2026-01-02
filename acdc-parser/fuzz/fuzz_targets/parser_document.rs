#![no_main]

use acdc_parser::{Options, parse};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, ignoring invalid UTF-8
    if let Ok(input) = std::str::from_utf8(data) {
        // Create default parsing options
        let options = Options::builder().build();

        // Attempt to parse the input
        // We don't care about the result, just that it doesn't panic or crash
        let _ = parse(input, &options);
    }
});
