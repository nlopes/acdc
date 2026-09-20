#![no_main]

use acdc_parser::{Options, parse};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, ignoring invalid UTF-8
    if let Ok(input) = std::str::from_utf8(data) {
        let options = Options::builder()
            .with_attributes([("myattr", "value"), ("version", "1.0")])
            .build()
            .expect("valid fuzz attributes");

        // Parse input which will exercise:
        // - Attribute reference substitutions
        // - Inline macro processing
        // - Passthrough handling
        // - Character replacements
        let _ = parse(input, &options);
    }
});
