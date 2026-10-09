use std::str::FromStr;

/// Controls include access and exposure of local paths during parsing.
///
/// Fixed resource limits apply in every mode. Output converters are responsible
/// for restrictions on stylesheets, scripts, and other rendered resources.
#[derive(Debug, Clone, Default, PartialOrd, PartialEq, Eq, Copy)]
pub enum SafeMode {
    /// Allows local includes without a base-directory boundary.
    ///
    /// HTTP(S) includes require caller-supplied `allow-uri-read`. The built-in
    /// transport also requires the `network` feature; custom providers supply
    /// their own transport. Include depth, response size, and other limits still apply.
    #[default]
    Unsafe = 0,

    /// Keeps local include paths beneath the effective include base directory.
    ///
    /// The base defaults to the entry file's parent, or the current directory for
    /// string and reader input. [`crate::OptionsBuilder::with_base_dir`] overrides it.
    /// For `/workspace/docs/main.adoc`, `../shared.adoc` becomes
    /// `/workspace/docs/shared.adoc`, while `/tmp/shared.adoc` becomes
    /// `/workspace/docs/tmp/shared.adoc`; both transformations emit a warning. This
    /// check does not resolve symlinks, so `/workspace/docs/linked.adoc` may point
    /// outside the directory. HTTP(S) includes require caller permission as in Unsafe mode.
    Safe,

    /// Applies Safe mode's include boundary and hides local directory and home paths.
    ///
    /// `docfile` exposes the entry filename instead of its full path. HTTP(S)
    /// includes require caller permission as in Unsafe mode.
    Server,

    /// Disables local and remote include reads, and hides paths as in Server mode.
    ///
    /// Include directives become link fallbacks with source-recovery warnings.
    /// [`crate::parse_file`] can still read the entry file selected by the caller.
    Secure,
}

impl FromStr for SafeMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "unsafe" => Ok(Self::Unsafe),
            "safe" => Ok(Self::Safe),
            "server" => Ok(Self::Server),
            "secure" => Ok(Self::Secure),
            _ => Err(format!(
                "invalid safe mode: '{s}', expected: unsafe, safe, server, secure"
            )),
        }
    }
}

impl SafeMode {
    #[must_use]
    pub const fn level(self) -> u8 {
        match self {
            Self::Unsafe => 0,
            Self::Safe => 1,
            Self::Server => 10,
            Self::Secure => 20,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unsafe => "unsafe",
            Self::Safe => "safe",
            Self::Server => "server",
            Self::Secure => "secure",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str() -> Result<(), String> {
        assert_eq!(SafeMode::from_str("unsafe")?, SafeMode::Unsafe);
        assert_eq!(SafeMode::from_str("UNSAFE")?, SafeMode::Unsafe);
        assert_eq!(SafeMode::from_str("safe")?, SafeMode::Safe);
        assert_eq!(SafeMode::from_str("server")?, SafeMode::Server);
        assert_eq!(SafeMode::from_str("secure")?, SafeMode::Secure);
        assert!(SafeMode::from_str("invalid").is_err());
        Ok(())
    }

    #[test]
    fn test_ordering() {
        assert!(SafeMode::Unsafe < SafeMode::Safe);
        assert!(SafeMode::Safe < SafeMode::Server);
        assert!(SafeMode::Server < SafeMode::Secure);
    }
}
