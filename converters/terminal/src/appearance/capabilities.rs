/// Terminal capability detection
#[derive(Debug, Clone)]
pub struct Capabilities {
    /// Terminal supports Unicode box drawing characters
    pub unicode: bool,
    /// Terminal supports OSC 8 hyperlinks
    pub osc8_links: bool,
}

impl Capabilities {
    /// Detect terminal capabilities from environment
    #[must_use]
    pub fn detect() -> Self {
        Self {
            unicode: Self::detect_unicode(),
            osc8_links: Self::detect_osc8(),
        }
    }

    /// Detect Unicode support from locale environment variables
    fn detect_unicode() -> bool {
        // Check LANG and LC_* environment variables for UTF-8
        for var in ["LANG", "LC_ALL", "LC_CTYPE"] {
            if let Ok(value) = std::env::var(var) {
                let value_upper = value.to_uppercase();
                if value_upper.contains("UTF-8") || value_upper.contains("UTF8") {
                    return true;
                }
            }
        }

        // Default to assuming Unicode support (most modern terminals)
        true
    }

    /// Detect OSC 8 hyperlink support from TERM environment variable
    #[must_use]
    pub(crate) fn detect_osc8() -> bool {
        if let Ok(term) = std::env::var("TERM") {
            let term_lower = term.to_lowercase();
            // Terminals known to support OSC 8
            return term_lower.contains("kitty")
                || term_lower.contains("wezterm")
                || term_lower.contains("ghostty")
                || term_lower.contains("iterm")
                || term_lower.contains("gnome")
                || term_lower.contains("konsole")
                || term_lower.contains("alacritty")
                || term_lower.contains("foot");
        }

        // Conservative default: don't assume OSC 8 support
        false
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::detect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_detection() {
        // Just verify detection completes without panicking
        let _caps = Capabilities::detect();
    }

    #[test]
    fn test_unicode_detection() {
        // Just test that detection runs without panicking
        // Actual value depends on environment
        let _has_unicode = Capabilities::detect_unicode();
    }

    #[test]
    fn test_osc8_detection() {
        // Just test that detection runs without panicking
        // Actual value depends on environment
        let _has_osc8 = Capabilities::detect_osc8();
    }
}
