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
    pub(crate) theme: Theme,
    /// Terminal capabilities (Unicode, OSC 8, etc.)
    pub capabilities: Capabilities,
    /// Color scheme based on theme
    pub(crate) colors: ColorScheme,
}

impl Appearance {
    /// Detect appearance settings from terminal environment
    #[must_use]
    pub fn detect() -> Self {
        let theme = Theme::detect();
        let capabilities = Capabilities::detect();
        let colors = ColorScheme::for_theme(theme);
        Self {
            theme,
            capabilities,
            colors,
        }
    }
}
