//! Link-target display utilities.

/// The visible fallback for a URL or `link:` macro with no explicit text.
///
/// A `mailto:` target is kept intact unless `hide_uri_scheme` is active. The
/// dedicated `mailto:` macro uses [`mailto_fallback`] instead.
#[must_use]
pub fn link_fallback(target: &str, hide_uri_scheme: bool) -> &str {
    if hide_uri_scheme {
        strip_uri_scheme(target)
    } else {
        target
    }
}

/// The visible fallback for a `mailto:` macro with no explicit text.
#[must_use]
pub fn mailto_fallback(target: &str) -> &str {
    target.strip_prefix("mailto:").unwrap_or(target)
}

/// The visible fallback for an automatically detected link.
///
/// The boolean reports whether the converter must put angle brackets around
/// the link. Asciidoctor keeps them for bracketed email addresses, but not for
/// bracketed URLs.
#[must_use]
pub fn autolink_fallback(target: &str, bracketed: bool, hide_uri_scheme: bool) -> (&str, bool) {
    match target.strip_prefix("mailto:") {
        Some(address) => (address, bracketed),
        None => (link_fallback(target, hide_uri_scheme), false),
    }
}

/// Remove a leading URI scheme and up to two following slashes.
#[must_use]
pub fn strip_uri_scheme(target: &str) -> &str {
    let Some(colon) = target.find(':') else {
        return target;
    };
    let scheme = &target[..colon];
    let Some((first, rest)) = scheme.as_bytes().split_first() else {
        return target;
    };
    if rest.is_empty()
        || !first.is_ascii_alphabetic()
        || !rest
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
    {
        return target;
    }

    let Some(remainder) = target.get(colon + 1..) else {
        return target;
    };
    remainder
        .strip_prefix("//")
        .or_else(|| remainder.strip_prefix('/'))
        .unwrap_or(remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_macros_only_strip_mailto_when_requested() {
        assert_eq!(
            link_fallback("mailto:user@example.com", false),
            "mailto:user@example.com"
        );
        assert_eq!(
            link_fallback("mailto:user@example.com", true),
            "user@example.com"
        );
        assert_eq!(
            mailto_fallback("mailto:user@example.com"),
            "user@example.com"
        );
    }

    #[test]
    fn only_bracketed_email_autolinks_keep_angle_brackets() {
        assert_eq!(
            autolink_fallback("mailto:user@example.com", true, false),
            ("user@example.com", true)
        );
        assert_eq!(
            autolink_fallback("https://example.com", true, false),
            ("https://example.com", false)
        );
    }

    #[test]
    fn strips_uri_prefixes_like_asciidoctor() {
        for (target, expected) in [
            ("https://example.com", "example.com"),
            ("ftp://files.example.com/a", "files.example.com/a"),
            ("mailto:user@example.com", "user@example.com"),
            ("tel:+1234", "+1234"),
            ("urn:isbn:1234", "isbn:1234"),
            ("scheme:/path", "path"),
            ("scheme:///path", "/path"),
        ] {
            assert_eq!(strip_uri_scheme(target), expected, "target: {target}");
        }
    }

    #[test]
    fn keeps_values_without_a_uri_scheme() {
        for target in [
            "example.com",
            "/path",
            "1bad:value",
            "x:value",
            "bad_:value",
        ] {
            assert_eq!(strip_uri_scheme(target), target);
        }
    }
}
