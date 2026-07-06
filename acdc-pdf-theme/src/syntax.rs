//! The bundled dark syntax-highlighting theme for code blocks.

/// The theme asset's crate-root-relative location, spelled once so the VFS path
/// and the `include_bytes!` that embeds it can't drift. `include_bytes!` needs a
/// literal, so this is a macro rather than a `const`.
macro_rules! highlight_theme_asset {
    () => {
        "assets/highlight.tmTheme"
    };
}

/// The virtual path the highlight theme is registered at (and referenced from
/// the generated `#set raw(theme: …)`). Shared so the emitter and renderer agree.
pub const HIGHLIGHT_THEME_PATH: &str = concat!("/", highlight_theme_asset!());

/// The bundled dark `tmTheme` bytes, for readable highlighting on the dark code
/// background.
#[must_use]
pub fn highlight_theme() -> &'static [u8] {
    include_bytes!(concat!("../", highlight_theme_asset!()))
}
