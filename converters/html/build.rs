//! Computes the CSP `script-src` hashes for the inline scripts acdc embeds, so
//! they never drift from the script contents.
//!
//! Each hash is the sha256 of a `static/*.js` file, base64-encoded and prefixed
//! with the algorithm (`sha256-…`). That is exactly the source a browser expects
//! in `script-src` for the matching inline `<script>`, whose body is the same
//! file embedded verbatim via `include_str!`. The values are exposed to the crate
//! through `cargo::rustc-env`, read back with `env!`.

use std::{env, error::Error, fs, path::Path};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn Error>> {
    emit_csp_hash(
        "static/terminal-replay-player.js",
        "ACDC_REPLAY_PLAYER_CSP_HASH",
    )?;
    emit_csp_hash("static/mathjax-config.js", "ACDC_MATHJAX_CONFIG_CSP_HASH")?;
    Ok(())
}

/// Hash the script at `relative_path` and expose `sha256-…` as `env_var`.
fn emit_csp_hash(relative_path: &str, env_var: &str) -> Result<(), Box<dyn Error>> {
    // Only recompute when the embedded script changes.
    println!("cargo::rerun-if-changed={relative_path}");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;
    let path = Path::new(&manifest_dir).join(relative_path);
    let bytes = fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    // Hash the exact bytes that appear between the <script> tags at runtime.
    let hash = format!("sha256-{}", STANDARD.encode(Sha256::digest(&bytes)));
    println!("cargo::rustc-env={env_var}={hash}");
    Ok(())
}
