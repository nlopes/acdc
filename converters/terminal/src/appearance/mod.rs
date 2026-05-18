mod capabilities;
mod theme;

pub use capabilities::Capabilities;
use theme::{ColorScheme, Theme};

/// Terminal appearance configuration.
///
/// Groups theme, capabilities, and color scheme together as they're
/// all related to how content is presented in the terminal.
#[derive(Clone, Debug)]
pub struct Appearance {
    /// Terminal theme (dark or light background)
    /// Used by `syntect_theme()` when `highlighting` feature is enabled.
    #[cfg(feature = "highlighting")]
    pub(crate) theme: Theme,
    /// Terminal capabilities (Unicode, OSC 8, etc.)
    pub(crate) capabilities: Capabilities,
    /// Color scheme based on theme
    pub(crate) colors: ColorScheme,
}

impl Appearance {
    /// Detect appearance settings from terminal environment
    #[must_use]
    pub(crate) fn detect() -> Self {
        let theme = Theme::detect();
        Self::for_theme(theme)
    }

    #[must_use]
    pub(crate) fn for_dark_mode(dark_mode: bool) -> Self {
        Self::for_theme(if dark_mode { Theme::Dark } else { Theme::Light })
    }

    #[must_use]
    fn for_theme(theme: Theme) -> Self {
        let capabilities = Capabilities::detect();
        let colors = ColorScheme::for_theme(theme);
        Self {
            #[cfg(feature = "highlighting")]
            theme,
            capabilities,
            colors,
        }
    }
}
