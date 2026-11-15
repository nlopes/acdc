//! Shared list rendering helpers.
//!
//! Ordered lists may carry an explicit numbering style (`[upperalpha]`,
//! `[lowerroman]`, …). Without one, the style follows the nesting level of the
//! list marker. Every backend resolves the style with [`OrderedListNumbering`].
//! Converters that render the marker as literal text (the terminal and manpage
//! backends) also use it to turn a 1-based item position into the styled
//! marker. The HTML backend instead maps the style to a CSS class and
//! `<ol type>`.

/// The numbering style of an ordered list, resolved from its block style
/// attribute. Unknown or absent styles resolve to [`Arabic`](Self::Arabic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrderedListNumbering {
    /// `1, 2, 3` — the default.
    #[default]
    Arabic,
    /// `1, 2, 3` — distinct CSS class in HTML, but identical marker text here.
    Decimal,
    /// `a, b, c`.
    LowerAlpha,
    /// `A, B, C`.
    UpperAlpha,
    /// `i, ii, iii`.
    LowerRoman,
    /// `I, II, III`.
    UpperRoman,
    /// `α, β, γ`.
    LowerGreek,
}

const LOWER_ALPHA: [u8; 26] = *b"abcdefghijklmnopqrstuvwxyz";
const LOWER_GREEK: [char; 24] = [
    'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 'λ', 'μ', 'ν', 'ξ', 'ο', 'π', 'ρ', 'σ', 'τ',
    'υ', 'φ', 'χ', 'ψ', 'ω',
];
const ROMAN: [(usize, &str); 13] = [
    (1000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
];

impl OrderedListNumbering {
    /// The default style for a list at nesting `depth` (1 = top level), as in
    /// asciidoctor: arabic, loweralpha, lowerroman, upperalpha, upperroman,
    /// then arabic for every deeper level.
    #[must_use]
    pub const fn for_depth(depth: usize) -> Self {
        match depth {
            2 => Self::LowerAlpha,
            3 => Self::LowerRoman,
            4 => Self::UpperAlpha,
            5 => Self::UpperRoman,
            _ => Self::Arabic,
        }
    }

    /// The nesting depth that an ordered list `marker` such as `..` denotes.
    /// asciidoctor also derives the default style from the marker rather than
    /// from the actual nesting.
    #[must_use]
    pub fn marker_depth(marker: &str) -> usize {
        marker.matches('.').count().max(1)
    }

    /// Resolve a list's numbering from its block `style` and its `marker`.
    /// An absent style uses the default for the marker's depth. An unknown
    /// style falls back to [`Arabic`](Self::Arabic).
    #[must_use]
    pub fn resolve(style: Option<&str>, marker: &str) -> Self {
        match style {
            Some(style) => Self::from_explicit_style(style).unwrap_or_default(),
            None => Self::for_depth(Self::marker_depth(marker)),
        }
    }

    /// Parse an explicit numbering-style name (a block style attribute value),
    /// returning [`None`] for any unrecognized name — the single source of truth
    /// for which style names exist.
    ///
    /// Most callers want [`resolve`](Self::resolve). Callers that must treat an
    /// unknown style differently from an absent one, such as the HTML backend,
    /// which keeps an unknown style as the CSS class, keep the [`Option`].
    #[must_use]
    pub fn from_explicit_style(style: &str) -> Option<Self> {
        match style {
            "arabic" => Some(Self::Arabic),
            "decimal" => Some(Self::Decimal),
            "loweralpha" => Some(Self::LowerAlpha),
            "upperalpha" => Some(Self::UpperAlpha),
            "lowerroman" => Some(Self::LowerRoman),
            "upperroman" => Some(Self::UpperRoman),
            "lowergreek" => Some(Self::LowerGreek),
            _ => None,
        }
    }

    /// Format a 1-based item `number` as its marker text, without any trailing
    /// punctuation (e.g. `4` in `UpperAlpha` → `"D"`, in `LowerRoman` → `"iv"`).
    #[must_use]
    pub fn format(self, number: usize) -> String {
        match self {
            Self::Arabic | Self::Decimal => number.to_string(),
            Self::LowerAlpha => bijective(number, &LOWER_ALPHA),
            Self::UpperAlpha => bijective(number, &LOWER_ALPHA).to_ascii_uppercase(),
            Self::LowerRoman => roman(number),
            Self::UpperRoman => roman(number).to_ascii_uppercase(),
            Self::LowerGreek => greek(number),
        }
    }
}

/// Bijective base-N over `alphabet` (1 → first letter, N → last, N+1 → "aa").
fn bijective(mut n: usize, alphabet: &[u8]) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let base = alphabet.len();
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(char::from(alphabet.get(n % base).copied().unwrap_or(b'?')));
        n /= base;
    }
    out.iter().rev().collect()
}

/// Lowercase roman numeral. Values outside the representable range fall back to
/// the decimal form (matching what a reader can still make sense of).
fn roman(mut n: usize) -> String {
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    let mut out = String::new();
    for (value, symbol) in ROMAN {
        while n >= value {
            out.push_str(symbol);
            n -= value;
        }
    }
    out
}

/// Bijective sequence over the 24 lowercase Greek letters.
fn greek(mut n: usize) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let base = LOWER_GREEK.len();
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(LOWER_GREEK.get(n % base).copied().unwrap_or('?'));
        n /= base;
    }
    out.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::OrderedListNumbering as N;

    #[test]
    fn parses_explicit_style() {
        assert_eq!(N::from_explicit_style("arabic"), Some(N::Arabic));
        assert_eq!(N::from_explicit_style("upperalpha"), Some(N::UpperAlpha));
        assert_eq!(N::from_explicit_style("lowergreek"), Some(N::LowerGreek));
        // Unrecognized names return `None` (distinct from explicit `arabic`).
        assert_eq!(N::from_explicit_style("bogus"), None);
    }

    #[test]
    fn defaults_to_arabic() {
        let resolve = |s: Option<&str>| s.and_then(N::from_explicit_style).unwrap_or_default();
        assert_eq!(resolve(None), N::Arabic);
        assert_eq!(resolve(Some("bogus")), N::Arabic);
        assert_eq!(resolve(Some("upperroman")), N::UpperRoman);
    }

    #[test]
    fn defaults_by_depth_like_asciidoctor() {
        let styles: Vec<N> = (1..=7).map(N::for_depth).collect();
        assert_eq!(
            styles,
            [
                N::Arabic,
                N::LowerAlpha,
                N::LowerRoman,
                N::UpperAlpha,
                N::UpperRoman,
                N::Arabic,
                N::Arabic,
            ]
        );
    }

    #[test]
    fn resolves_style_from_marker_depth() {
        assert_eq!(N::resolve(None, "."), N::Arabic);
        assert_eq!(N::resolve(None, ".."), N::LowerAlpha);
        assert_eq!(N::resolve(None, "....."), N::UpperRoman);
        assert_eq!(N::resolve(None, "......"), N::Arabic);
        assert_eq!(N::resolve(None, "1."), N::Arabic);
        assert_eq!(N::resolve(Some("lowergreek"), ".."), N::LowerGreek);
        assert_eq!(N::resolve(Some("bogus"), ".."), N::Arabic);
    }

    #[test]
    fn formats_arabic_and_decimal() {
        assert_eq!(N::Arabic.format(1), "1");
        assert_eq!(N::Decimal.format(42), "42");
    }

    #[test]
    fn formats_alpha() {
        assert_eq!(N::LowerAlpha.format(1), "a");
        assert_eq!(N::LowerAlpha.format(26), "z");
        assert_eq!(N::LowerAlpha.format(27), "aa");
        assert_eq!(N::UpperAlpha.format(2), "B");
        assert_eq!(N::UpperAlpha.format(28), "AB");
    }

    #[test]
    fn formats_roman() {
        assert_eq!(N::LowerRoman.format(1), "i");
        assert_eq!(N::LowerRoman.format(4), "iv");
        assert_eq!(N::LowerRoman.format(9), "ix");
        assert_eq!(N::UpperRoman.format(14), "XIV");
        assert_eq!(N::UpperRoman.format(2024), "MMXXIV");
    }

    #[test]
    fn formats_greek() {
        assert_eq!(N::LowerGreek.format(1), "α");
        assert_eq!(N::LowerGreek.format(24), "ω");
        assert_eq!(N::LowerGreek.format(25), "αα");
    }
}
