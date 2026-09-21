#![no_main]

use std::io::Cursor;

use acdc_parser::{Options, SafeMode, parse_from_reader};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, ignoring invalid UTF-8
    if let Ok(input) = std::str::from_utf8(data) {
        // Secure mode keeps includes from reading files or URLs.
        let options = Options::builder()
            .with_safe_mode(SafeMode::Secure)
            .build()
            .expect("valid fuzz options");

        let mut reader = Cursor::new(input.as_bytes());
        let _ = parse_from_reader(&mut reader, &options);
    }
});
