//! Bundled, open-licensed fonts (IBM Plex and Noto Color Emoji, both SIL OFL 1.1).

/// The family name of the bundled emoji face.
pub const EMOJI_FONT_FAMILY: &str = "Noto Color Emoji";

macro_rules! face {
    ($file:literal) => {
        include_bytes!(concat!("../assets/fonts/", $file)) as &'static [u8]
    };
}

/// Every bundled font face, as raw ttf bytes, ready to hand to a font engine.
#[must_use]
pub fn embedded_fonts() -> &'static [&'static [u8]] {
    &[
        face!("IBMPlexSans-Regular.ttf"),
        face!("IBMPlexSans-Italic.ttf"),
        face!("IBMPlexSans-Medium.ttf"),
        face!("IBMPlexSans-SemiBold.ttf"),
        face!("IBMPlexSans-Bold.ttf"),
        face!("IBMPlexSerif-Regular.ttf"),
        face!("IBMPlexSerif-Medium.ttf"),
        face!("IBMPlexSerif-Bold.ttf"),
        face!("IBMPlexSerif-Italic.ttf"),
        face!("IBMPlexMono-Regular.ttf"),
        face!("IBMPlexMono-Bold.ttf"),
        face!("NotoColorEmoji.ttf"),
    ]
}
