#![no_main]

use acdc_parser::{AttributeValue, DocumentAttributes, Options, parse};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, ignoring invalid UTF-8
    if let Ok(input) = std::str::from_utf8(data) {
        // Create options with various attribute combinations that trigger preprocessing
        let mut attributes = DocumentAttributes::default();

        // Add some attributes that might trigger substitutions
        attributes.insert(
            "myattr".to_string(),
            AttributeValue::String("value".to_string()),
        );
        attributes.insert(
            "version".to_string(),
            AttributeValue::String("1.0".to_string()),
        );

        let options = Options::builder().with_attributes(attributes).build();

        // Parse input which will exercise:
        // - Attribute reference substitutions
        // - Inline macro processing
        // - Passthrough handling
        // - Character replacements
        let _ = parse(input, &options);
    }
});
