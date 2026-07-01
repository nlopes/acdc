//! Content Security Policy construction for generated HTML.
//!
//! acdc emits self-contained HTML and, by default, sets no CSP. When the `:csp:`
//! document attribute is set, standalone output carries a
//! `<meta http-equiv="Content-Security-Policy">` built by
//! [`content_security_policy`]. The same builder is public so embedded-mode
//! consumers (who own their own `<head>`) can reproduce acdc's policy.
//!
//! A CSP is a restriction, not an enabler: the policy locks scripts down to
//! acdc's own inline scripts (by hash, no `'unsafe-inline'`) plus the `MathJax`
//! CDN, while keeping passive resources (images, media, video embeds) permissive
//! so arbitrary document content keeps loading.

/// Which acdc features a document uses, so the right sources end up in its Content
/// Security Policy. Defaults to all `false`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent on/off feature flags for selecting CSP sources"
)]
pub struct CspFeatures {
    /// `:stem:` is set, so the inline `MathJax` config and the `MathJax` CDN loader are
    /// emitted.
    pub stem: bool,
    /// Webfonts are linked (Google Fonts CSS + font files).
    pub webfonts: bool,
    /// `icons=font` is set, so the Font Awesome stylesheet (and its fonts) are loaded
    /// from the CDN.
    pub icons_font: bool,
    /// A  terminal replay player  may be present, so  its inline script  is allow-listed.
    /// Ignored unless the `terminal` feature is compiled in.
    pub replay: bool,
}

/// Build the Content Security Policy value (the `content="..."` string) for the
/// given features.
///
/// `script-src` allow-lists acdc's inline scripts by hash with no
/// `'unsafe-inline'`; `style-src` keeps `'unsafe-inline'` because acdc emits
/// inline `style=` widely (e.g. terminal cell colours from recorded data, which
/// cannot be hashed). Passive directives stay permissive so remote images,
/// media, and video embeds in document content still load. `frame-ancestors` is
/// omitted on purpose: it is ignored in a `<meta>` CSP and must be a response
/// header.
#[must_use]
pub fn content_security_policy(features: &CspFeatures) -> String {
    let mut script_src = String::from("script-src 'self'");
    #[cfg(feature = "terminal")]
    if features.replay {
        script_src.push_str(" '");
        script_src.push_str(crate::REPLAY_PLAYER_SCRIPT_CSP_HASH);
        script_src.push('\'');
    }
    if features.stem {
        script_src.push_str(" '");
        script_src.push_str(crate::MATHJAX_CONFIG_CSP_HASH);
        script_src.push_str("' https://cdn.jsdelivr.net");
    }

    let mut style_src = String::from("style-src 'self' 'unsafe-inline'");
    if features.webfonts {
        style_src.push_str(" https://fonts.googleapis.com");
    }
    if features.icons_font {
        style_src.push_str(" https://cdn.jsdelivr.net");
    }

    let mut font_src = String::from("font-src 'self'");
    if features.webfonts {
        font_src.push_str(" https://fonts.gstatic.com");
    }
    if features.icons_font {
        font_src.push_str(" https://cdn.jsdelivr.net");
    }

    [
        "default-src 'self'",
        &script_src,
        &style_src,
        &font_src,
        "img-src 'self' data: https:",
        "media-src 'self' https:",
        "frame-src https://www.youtube.com https://player.vimeo.com",
    ]
    .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_locks_scripts_without_unsafe_inline() {
        let policy = content_security_policy(&CspFeatures::default());
        assert!(policy.contains("default-src 'self'"));
        assert!(policy.contains("script-src 'self'"));
        // Scripts are never allowed inline wholesale.
        assert!(!policy.contains("script-src 'self' 'unsafe-inline'"));
        // Styles must allow inline (acdc emits inline style attributes).
        assert!(policy.contains("style-src 'self' 'unsafe-inline'"));
        // Passive resources stay permissive so document content keeps loading.
        assert!(policy.contains("img-src 'self' data: https:"));
        assert!(policy.contains("media-src 'self' https:"));
        // No CDN sources without the features that need them.
        assert!(!policy.contains("cdn.jsdelivr.net"));
        assert!(!policy.contains("fonts.googleapis.com"));
    }

    #[test]
    fn stem_adds_mathjax_hash_and_cdn() {
        let policy = content_security_policy(&CspFeatures {
            stem: true,
            ..CspFeatures::default()
        });
        assert!(policy.contains(super::super::MATHJAX_CONFIG_CSP_HASH));
        assert!(policy.contains("script-src 'self'"));
        assert!(policy.contains("https://cdn.jsdelivr.net"));
    }

    #[test]
    fn webfonts_and_icons_add_font_hosts() {
        let policy = content_security_policy(&CspFeatures {
            webfonts: true,
            icons_font: true,
            ..CspFeatures::default()
        });
        assert!(policy.contains("style-src 'self' 'unsafe-inline' https://fonts.googleapis.com"));
        assert!(policy.contains("https://fonts.gstatic.com"));
        // Font Awesome's stylesheet and fonts both come from jsDelivr.
        assert!(
            policy.contains("font-src 'self' https://fonts.gstatic.com https://cdn.jsdelivr.net")
        );
    }

    #[cfg(feature = "terminal")]
    #[test]
    fn replay_adds_player_hash() {
        let policy = content_security_policy(&CspFeatures {
            replay: true,
            ..CspFeatures::default()
        });
        assert!(policy.contains(crate::REPLAY_PLAYER_SCRIPT_CSP_HASH));
    }
}
