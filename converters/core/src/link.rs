//! Link-target display utilities.

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
