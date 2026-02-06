#!/bin/bash
# Regenerate expected Markdown outputs from AsciiDoc fixtures
# Usage: ./regenerate_expected.sh [fixture_name]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/../../.."
ACDC_BIN="$PROJECT_ROOT/target/debug/acdc"

# Build if needed
if [ ! -f "$ACDC_BIN" ]; then
    echo "Building acdc..."
    cd "$PROJECT_ROOT"
    cargo build --features markdown --quiet
fi

SOURCE_DIR="$SCRIPT_DIR/fixtures/source"
EXPECTED_DIR="$SCRIPT_DIR/fixtures/expected"

if [ -n "$1" ]; then
    # Regenerate specific fixture
    FIXTURE="$1"
    echo "Regenerating expected output for: $FIXTURE"
    "$ACDC_BIN" convert --backend markdown \
        "$SOURCE_DIR/${FIXTURE}.adoc" \
        -o "$EXPECTED_DIR/${FIXTURE}.md"
    echo "✓ Updated $EXPECTED_DIR/${FIXTURE}.md"
else
    # Regenerate all fixtures
    echo "Regenerating all expected outputs..."
    for adoc_file in "$SOURCE_DIR"/*.adoc; do
        basename=$(basename "$adoc_file" .adoc)
        echo "  Processing: $basename"
        "$ACDC_BIN" convert --backend markdown \
            "$adoc_file" \
            -o "$EXPECTED_DIR/${basename}.md" 2>/dev/null || true
    done
    echo "✓ All expected outputs updated"
fi

echo "Done!"
