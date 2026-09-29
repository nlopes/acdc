//! Link destinations and fallback labels.

use std::fmt::Write as _;

use acdc_parser::Mailto;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

const MAILTO_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Build a mailto destination from its address and positional subject/body.
///
/// Keep the original target for the visible fallback. Encode query values here,
/// after the parser has restored passthroughs, and leave output escaping to the
/// converter. Unlike Asciidoctor, this preserves literal ampersands and extends
/// an existing query without introducing a second question mark.
#[must_use]
pub fn mailto_target(mailto: &Mailto<'_>) -> String {
    let mut target = mailto.target.to_string();
    if !target.starts_with("mailto:") {
        target.insert_str(0, "mailto:");
    }
    if let Some(subject) = mailto.subject {
        // Positional arguments override matching headers in an existing query;
        // other headers retain their original spelling and encoding.
        if let Some((address, query)) = target.split_once('?') {
            let query = query
                .split('&')
                .filter(|field| {
                    let name = field.split_once('=').map_or(*field, |(name, _)| name);
                    let name = percent_decode_str(name).decode_utf8_lossy();
                    !(field.is_empty()
                        || name.eq_ignore_ascii_case("subject")
                        || mailto.body.is_some() && name.eq_ignore_ascii_case("body"))
                })
                .collect::<Vec<_>>()
                .join("&");
            target = if query.is_empty() {
                address.to_string()
            } else {
                format!("{address}?{query}")
            };
        }
        let separator = if target.contains('?') { "&" } else { "?" };
        let _ = write!(
            target,
            "{separator}subject={}",
            utf8_percent_encode(subject, MAILTO_COMPONENT)
        );
        if let Some(body) = mailto.body {
            // RFC 6068 requires CRLF for message-body line breaks.
            let body = body.replace("\r\n", "\n").replace(['\r', '\n'], "\r\n");
            let _ = write!(
                target,
                "&body={}",
                utf8_percent_encode(&body, MAILTO_COMPONENT)
            );
        }
    }
    target
}

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
    fn mailto_queries_preserve_other_headers_and_replace_matching_headers()
    -> Result<(), Box<dyn std::error::Error>> {
        for (source, expected) in [
            (
                "mailto:a@example.org?cc=b@example.org&[Mail,New]",
                "mailto:a@example.org?cc=b@example.org&subject=New",
            ),
            (
                "mailto:a@example.org?SUBJECT=Old&%62ody=Old&cc=b@example.org[Mail,New,New body]",
                "mailto:a@example.org?cc=b@example.org&subject=New&body=New%20body",
            ),
            (
                "mailto:a@example.org?body=Keep[Mail,New]",
                "mailto:a@example.org?body=Keep&subject=New",
            ),
            (
                "mailto:a@example.org?subject=Keep[Mail]",
                "mailto:a@example.org?subject=Keep",
            ),
            (
                "mailto:a@example.org?[Mail,New]",
                "mailto:a@example.org?subject=New",
            ),
            (
                "mailto:a@example.org[Mail,Subject,\"First\nsecond\"]",
                "mailto:a@example.org?subject=Subject&body=First%0D%0Asecond",
            ),
        ] {
            let parsed = acdc_parser::parse_inline(source, &acdc_parser::Options::default())?;
            let [acdc_parser::InlineNode::Macro(acdc_parser::InlineMacro::Mailto(mailto))] =
                parsed.inlines()
            else {
                return Err(format!("expected mailto: {source}").into());
            };
            assert_eq!(mailto_target(mailto), expected, "{source}");
        }
        Ok(())
    }

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
