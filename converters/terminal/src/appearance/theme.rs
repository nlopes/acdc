use crossterm::style::Color;

/// Terminal background theme
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Theme {
    Dark,
    Light,
}

impl Theme {
    /// Detect theme from terminal environment
    ///
    /// Checks COLORFGBG environment variable which is set by many terminals.
    /// Format is "foreground;background" where higher numbers indicate lighter colors.
    /// Falls back to Dark theme if detection fails.
    #[must_use]
    pub fn detect() -> Self {
        if let Ok(colorfgbg) = std::env::var("COLORFGBG") {
            // Parse format like "15;0" (white on black) or "0;15" (black on white)
            if let Some(bg) = colorfgbg.split(';').nth(1)
                && let Ok(bg_num) = bg.parse::<u8>()
            {
                // Background colors 0-7 are typically dark
                // Background colors 8-15 are typically light
                return if bg_num >= 8 { Self::Light } else { Self::Dark };
            }
        }

        // Default to dark theme
        Self::Dark
    }

    /// Get the appropriate syntax highlighting theme name
    #[cfg(feature = "highlighting")]
    #[must_use]
    pub const fn syntect_theme(self) -> &'static str {
        match self {
            Self::Dark => "Solarized (dark)",
            Self::Light => "Solarized (light)",
        }
    }
}

/// Semantic color scheme for terminal output
#[derive(Debug, Clone)]
pub(crate) struct ColorScheme {
    // Section colors
    pub section_h1: Color,
    pub section_h2: Color,
    pub section_h3: Color,
    pub section_h4: Color,
    pub section_h5: Color,
    pub section_h6: Color,

    // Block type labels
    pub label_listing: Color,

    // Admonitions
    pub admon_note: Color,
    pub admon_tip: Color,
    pub admon_important: Color,
    pub admon_warning: Color,
    pub admon_caution: Color,

    // Inline elements
    pub inline_monospace: Color,

    // Special elements
    pub footnote: Color,
    pub link: Color,
    pub table_header: comfy_table::Color,
    pub table_footer: comfy_table::Color,
}

impl ColorScheme {
    /// Create color scheme for dark terminal background
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            // Sections - varying intensities for hierarchy
            section_h1: Color::Rgb {
                r: 215,
                g: 0,
                b: 135,
            }, // Pink
            section_h2: Color::White,
            section_h3: Color::Cyan,
            section_h4: Color::Cyan,
            section_h5: Color::Blue,
            section_h6: Color::Grey,

            // Block labels - muted colors
            label_listing: Color::DarkGrey,

            // Admonitions - semantic colors
            admon_note: Color::Blue,
            admon_tip: Color::Green,
            admon_important: Color::Yellow,
            admon_warning: Color::Red,
            admon_caution: Color::Magenta,

            // Inline elements
            inline_monospace: Color::Cyan,

            // Special elements
            footnote: Color::Cyan,
            link: Color::Blue,
            table_header: comfy_table::Color::Green,
            table_footer: comfy_table::Color::Cyan,
        }
    }

    /// Create color scheme for light terminal background
    #[must_use]
    pub const fn light() -> Self {
        Self {
            // Sections - darker colors for light background
            section_h1: Color::Rgb {
                r: 180,
                g: 0,
                b: 100,
            }, // Darker Pink
            section_h2: Color::Black,
            section_h3: Color::DarkBlue,
            section_h4: Color::DarkBlue,
            section_h5: Color::DarkCyan,
            section_h6: Color::DarkGrey,

            // Block labels - darker muted colors
            label_listing: Color::Grey,

            // Admonitions - darker semantic colors
            admon_note: Color::DarkBlue,
            admon_tip: Color::DarkGreen,
            admon_important: Color::DarkYellow,
            admon_warning: Color::DarkRed,
            admon_caution: Color::DarkMagenta,

            // Inline elements
            inline_monospace: Color::DarkCyan,

            // Special elements
            footnote: Color::DarkCyan,
            link: Color::DarkBlue,
            table_header: comfy_table::Color::DarkGreen,
            table_footer: comfy_table::Color::DarkCyan,
        }
    }

    /// Get the appropriate color scheme for a theme
    #[must_use]
    pub const fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::dark(),
            Theme::Light => Self::light(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_detection() {
        // Just test that detection runs without panicking
        // Actual value depends on COLORFGBG environment variable
        let _theme = Theme::detect();
    }

    #[cfg(feature = "highlighting")]
    #[test]
    fn test_syntect_theme_names() {
        assert_eq!(Theme::Dark.syntect_theme(), "Solarized (dark)");
        assert_eq!(Theme::Light.syntect_theme(), "Solarized (light)");
    }

    #[test]
    fn test_color_scheme_creation() {
        let dark = ColorScheme::dark();
        let light = ColorScheme::light();

        // Verify they're different
        assert_ne!(
            format!("{:?}", dark.section_h1),
            format!("{:?}", light.section_h1)
        );
    }
}
