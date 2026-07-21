use std::str::FromStr;

/// Safe mode to use when processing the document. This follows from what is described in
/// <https://docs.asciidoctor.org/asciidoctor/latest/safe-modes/> and is intended to
/// provide similar functionality as Asciidoctor.
#[derive(Debug, Clone, Default, PartialOrd, PartialEq, Eq, Copy)]
pub enum SafeMode {
    /// The `UNSAFE` safe mode level disables all security measures.
    #[default]
    Unsafe = 0,

    /// The `SAFE` safe mode level keeps local include paths beneath the entry document's
    /// parent directory. For `/workspace/docs/main.adoc`, `../shared.adoc` becomes
    /// `/workspace/docs/shared.adoc`, while `/tmp/shared.adoc` becomes
    /// `/workspace/docs/tmp/shared.adoc`; both transformations emit a warning. This
    /// check does not resolve symlinks, so `/workspace/docs/linked.adoc` may point
    /// outside the directory. This mode allows assets (such as the stylesheet) to be
    /// embedded in the document.
    Safe,

    /// The `SERVER` safe mode level disallows the document from setting attributes that
    /// would affect conversion of the document. This level trims docfile to its relative
    /// path and prevents the document from:
    ///
    /// - setting source-highlighter, doctype, docinfo and backend
    /// - seeing docdir (as it can reveal information about the host filesystem)
    ///
    /// It applies the same local path transformations as [`SafeMode::Safe`]. It allows
    /// icons and linkcss. No includes from a url are allowed unless the `allow-uri-read`
    /// attribute is set.
    Server,

    /// The `SECURE` safe mode level disallows the document from attempting to read files
    /// from the file system and including their contents into the document. Additionally,
    /// it:
    ///
    /// - disables icons
    /// - disables include directives (`include::[]`)
    /// - data can not be retrieved from URIs
    /// - prevents access to stylesheets and JavaScript files
    /// - sets the backend to html5
    /// - disables docinfo files
    /// - disables data-uri
    /// - disables interactive (opts=interactive) and inline (opts=inline) modes for SVGs
    /// - disables docdir and docfile (as these can reveal information about the host
    ///   filesystem)
    /// - disables source highlighting
    ///
    /// Note: `GitHub` processes `AsciiDoc` files using the `SECURE` mode.
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
