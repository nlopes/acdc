# CLI — Developer Guide

## TCK compliance

The CLI supports the AsciiDoc TCK (Test Compatibility Kit) behind a feature flag.

Build: `cargo build --package acdc-cli --features tck`

Manual test:
```bash
echo '{"contents":"= Hello","path":"test.adoc","type":"block"}' | cargo run -p acdc-cli --features tck -- tck
```
