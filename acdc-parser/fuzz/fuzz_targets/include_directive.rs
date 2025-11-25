#![no_main]

use libfuzzer_sys::fuzz_target;
use acdc_core::SafeMode;
use acdc_parser::{parse_from_reader, DocumentAttributes, Options};
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, ignoring invalid UTF-8
    if let Ok(input) = std::str::from_utf8(data) {
        // Create options with safe mode enabled to prevent actual file system access
        // This tests the include directive parsing logic without security risks
        let options = Options {
            document_attributes: DocumentAttributes::default(),
            safe_mode: SafeMode::Safe,
            ..Default::default()
        };

        // Parse from reader which will exercise include directive handling
        // Safe mode prevents actual file system access during fuzzing
        let reader = Cursor::new(input.as_bytes());
        let _ = parse_from_reader(reader, &options);
    }
});
