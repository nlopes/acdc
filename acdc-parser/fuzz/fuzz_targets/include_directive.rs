#![no_main]

use std::io::Cursor;

use acdc_core::SafeMode;
use acdc_parser::{DocumentAttributes, Options, parse_from_reader};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, ignoring invalid UTF-8
    if let Ok(input) = std::str::from_utf8(data) {
        // Create options with safe mode enabled to prevent actual file system access
        // This tests the include directive parsing logic without security risks
        let options = Options::builder().with_safe_mode(SafeMode::Safe).build();

        // Parse from reader which will exercise include directive handling
        // Safe mode prevents actual file system access during fuzzing
        let reader = Cursor::new(input.as_bytes());
        let _ = parse_from_reader(reader, &options);
    }
});
