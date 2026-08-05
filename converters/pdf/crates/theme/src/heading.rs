use serde::Deserialize;

/// Controls whether a heading starts on a new page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageBreakBefore {
    /// Let the heading follow the preceding content.
    Auto,
    /// Start the heading on a new page.
    #[default]
    Always,
}

/// Controls the page break between a part and its first chapter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartBreakAfter {
    /// Follow the chapter's `break_before` policy.
    #[default]
    Auto,
    /// Start the first chapter on a new page.
    Always,
    /// Do not force a page break before the first chapter.
    Avoid,
}

/// Page-break settings for part headings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PartHeading {
    /// Controls whether each part starts on a new page.
    pub break_before: PageBreakBefore,
    /// Controls the break between a part and its first chapter.
    pub break_after: PartBreakAfter,
}

/// Page-break settings for chapter headings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChapterHeading {
    /// Controls whether each chapter starts on a new page.
    pub break_before: PageBreakBefore,
}

/// Book heading page-break settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Heading {
    /// Page-break settings for book parts.
    pub part: PartHeading,
    /// Page-break settings for book chapters.
    pub chapter: ChapterHeading,
}
