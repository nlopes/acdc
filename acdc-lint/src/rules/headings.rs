use acdc_parser::ast::{
    BlockKind, BlockRef, ContentRef, DocumentRef, InlineKind, LocationRef, StructuralRef,
};

use crate::LintId;

use super::{
    LintEmitter, SourceLine, delimiter_token, is_block_attribute_line, is_skipped_line,
    root_list_family, source_line_at, split_first_char,
};

pub(crate) fn lint_section_title_symmetric_marker(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
    lines: &[SourceLine<'_>],
) {
    if let Some(header) = document.header() {
        lint_symmetric_title_line(emitter, lines, header.location());
    }
    for_each_heading(StructuralRef::Document(document), &mut |block| {
        lint_symmetric_title_line(emitter, lines, block.location());
    });
}

fn for_each_heading<'doc>(node: StructuralRef<'doc>, visit: &mut impl FnMut(BlockRef<'doc>)) {
    if let StructuralRef::Block(block) = node {
        // Heading advice excludes table-cell content.
        if matches!(block.kind(), BlockKind::Table(_)) {
            return;
        }
        if matches!(
            block.kind(),
            BlockKind::Section(_) | BlockKind::DiscreteHeader(_)
        ) {
            visit(block);
        }
    }
    for child in node.children() {
        for_each_heading(child, visit);
    }
}

fn lint_symmetric_title_line(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    location: LocationRef<'_>,
) {
    let Some(line) = source_line_at(lines, location.start().line()) else {
        return;
    };
    if is_symmetric_atx_title(line.text.trim()) {
        emitter.emit(
            LintId::SectionTitleSymmetricMarker,
            "section title uses symmetric ATX markers",
            None,
            Some(emitter.point_location(line.number, 1)),
        );
    }
}

pub(crate) fn lint_section_title_setext_style(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    for pair in lines.windows(2) {
        let [title_line, underline_line] = pair else {
            continue;
        };
        if is_skipped_line(title_line.number, skipped_lines) {
            continue;
        }
        let title = title_line.text.trim();
        let underline = underline_line.text.trim();
        if is_setext_title_pair(title, underline) {
            emitter.emit(
                LintId::SectionTitleSetextStyle,
                "section title uses setext underline style",
                None,
                Some(emitter.point_location(underline_line.number, 1)),
            );
        }
    }
}

pub(crate) fn lint_section_title_marker_spacing(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    for line in lines {
        if is_skipped_line(line.number, skipped_lines) {
            continue;
        }

        let trimmed = line.text.trim_start();
        let Some(marker_len) = bad_heading_marker_len(trimmed) else {
            continue;
        };
        let leading_columns = line
            .text
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .count();
        emitter.emit(
            LintId::SectionTitleMarkerSpacing,
            "section title marker should be followed by whitespace",
            None,
            Some(emitter.point_location(
                line.number,
                leading_columns.saturating_add(marker_len).saturating_add(1),
            )),
        );
    }
}

fn bad_heading_marker_len(trimmed: &str) -> Option<usize> {
    let (marker, _) = split_first_char(trimmed)?;
    if !matches!(marker, '=' | '#') {
        return None;
    }

    let marker_len = trimmed.chars().take_while(|ch| *ch == marker).count();
    if !(1..=6).contains(&marker_len) {
        return None;
    }

    trimmed
        .chars()
        .nth(marker_len)
        .is_some_and(|ch| !ch.is_whitespace())
        .then_some(marker_len)
}

pub(crate) fn lint_section_title_capitalization(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
) {
    if let Some(header) = document.header()
        && let Some(title) = header.title()
    {
        lint_title_capitalization(emitter, title, "document title");
    }
    for_each_heading(StructuralRef::Document(document), &mut |block| {
        if let BlockKind::Section(section) = block.kind()
            && let Some(title) = section.title()
        {
            lint_title_capitalization(emitter, title, "section title");
        } else if let BlockKind::DiscreteHeader(header) = block.kind()
            && let Some(title) = header.title()
        {
            lint_title_capitalization(emitter, title, "discrete heading");
        }
    });
}

fn lint_title_capitalization(
    emitter: &mut LintEmitter<'_>,
    title: ContentRef<'_>,
    title_kind: &str,
) {
    let Some(first) = first_alphabetic_inlines(title) else {
        return;
    };
    if !first.character.is_lowercase() {
        return;
    }

    if first.in_monospace {
        emitter.emit(
            LintId::SectionTitleCapitalizationMonospace,
            format!("{title_kind} monospace text should start with an uppercase letter"),
            None,
            Some(emitter.source_location(first.location)),
        );
    } else {
        emitter.emit(
            LintId::SectionTitleCapitalization,
            format!("{title_kind} should start with an uppercase letter"),
            None,
            Some(emitter.source_location(first.location)),
        );
    }
}

#[derive(Clone, Copy)]
struct FirstAlphabetic<'doc> {
    character: char,
    location: LocationRef<'doc>,
    in_monospace: bool,
}

fn first_alphabetic_inlines(content: ContentRef<'_>) -> Option<FirstAlphabetic<'_>> {
    for node in content.inlines() {
        let found = match node.kind() {
            InlineKind::Plain(text) => first_alphabetic_text(text.text(), text.location()),
            InlineKind::Raw(text) => first_alphabetic_text(text.text(), text.location()),
            InlineKind::Verbatim(text) => first_alphabetic_text(text.text(), text.location()),
            InlineKind::Bold(text) => first_alphabetic_inlines(text.content()),
            InlineKind::CurvedApostrophe(text) => first_alphabetic_inlines(text.content()),
            InlineKind::CurvedQuotation(text) => first_alphabetic_inlines(text.content()),
            InlineKind::Highlight(text) => first_alphabetic_inlines(text.content()),
            InlineKind::Italic(text) => first_alphabetic_inlines(text.content()),
            InlineKind::Monospace(text) => {
                first_alphabetic_inlines(text.content()).map(FirstAlphabetic::with_monospace)
            }
            InlineKind::Subscript(text) => first_alphabetic_inlines(text.content()),
            InlineKind::Superscript(text) => first_alphabetic_inlines(text.content()),
            InlineKind::Button(button) => first_alphabetic_inlines(button.label()),
            InlineKind::CrossReference(reference) => first_alphabetic_inlines(reference.text()),
            InlineKind::Footnote(footnote) => first_alphabetic_inlines(footnote.content()),
            InlineKind::Link(link) => first_alphabetic_inlines(link.text()),
            InlineKind::Mailto(mailto) => first_alphabetic_inlines(mailto.text()),
            InlineKind::Menu(menu) => first_alphabetic_inlines(menu.target()),
            InlineKind::Pass(pass) => pass
                .text()
                .and_then(|text| first_alphabetic_text(text, pass.location())),
            InlineKind::Url(url) => first_alphabetic_inlines(url.text()),
            InlineKind::Autolink(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::IndexTerm(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Stem(_)
            | InlineKind::Callout(_)
            | InlineKind::Anchor(_)
            | InlineKind::LineBreak(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::Symbol(_)
            | InlineKind::CharacterReference(_)
            | _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

impl FirstAlphabetic<'_> {
    const fn with_monospace(self) -> Self {
        Self {
            in_monospace: true,
            ..self
        }
    }
}

fn first_alphabetic_text<'doc>(
    text: &str,
    location: LocationRef<'doc>,
) -> Option<FirstAlphabetic<'doc>> {
    text.chars()
        .find(|ch| ch.is_alphabetic())
        .map(|character| FirstAlphabetic {
            character,
            location,
            in_monospace: false,
        })
}

pub(crate) fn lint_document_title_author(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
    lines: &[SourceLine<'_>],
) {
    let Some(header) = document.header() else {
        return;
    };
    let header_lines = collect_header_lines(lines);

    if header.authors().next().is_none() {
        let line = header_lines.title.map_or_else(
            || line_after(header.location().end().line()),
            |line| line.saturating_add(1),
        );
        emitter.emit(
            LintId::DocumentTitleAuthor,
            "document title is missing an author line",
            None,
            Some(emitter.point_location(line, 1)),
        );
    }
}

pub(crate) fn lint_document_title_revision(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
    lines: &[SourceLine<'_>],
) {
    let Some(header) = document.header() else {
        return;
    };
    if header.authors().next().is_none() {
        return;
    }

    let header_lines = collect_header_lines(lines);

    let has_revision_attribute = document.attributes().get("revnumber").is_some()
        || document.attributes().get("revdate").is_some()
        || document.attributes().get("revremark").is_some();
    if has_revision_attribute || header_lines.revision.is_some() {
        return;
    }

    let line = header_lines.author.map_or_else(
        || line_after(header.location().end().line()),
        |line| line.saturating_add(1),
    );
    emitter.emit(
        LintId::DocumentTitleRevision,
        "document header is missing a revision line",
        None,
        Some(emitter.point_location(line, 1)),
    );
}

fn is_symmetric_atx_title(trimmed: &str) -> bool {
    let Some((marker, count)) = atx_marker(trimmed) else {
        return false;
    };
    let closing_marker = std::iter::repeat_n(marker, count).collect::<String>();
    let suffix = format!(" {closing_marker}");
    trimmed.ends_with(&suffix)
}

fn is_atx_title(trimmed: &str) -> bool {
    atx_marker(trimmed).is_some()
}

fn atx_marker(trimmed: &str) -> Option<(char, usize)> {
    let (marker, rest) = split_first_char(trimmed)?;
    if !matches!(marker, '=' | '#') {
        return None;
    }

    let count = trimmed.chars().take_while(|ch| *ch == marker).count();
    if !(1..=6).contains(&count) {
        return None;
    }

    let rest = rest.trim_start_matches(marker);
    rest.starts_with(char::is_whitespace)
        .then_some((marker, count))
}

fn is_setext_underline(trimmed: &str) -> bool {
    setext_underline_marker(trimmed).is_some()
}

fn is_setext_title_pair(title: &str, underline: &str) -> bool {
    is_setext_title_text(title)
        && is_setext_underline(underline)
        && title.chars().count().abs_diff(underline.chars().count()) <= 2
}

fn setext_underline_marker(trimmed: &str) -> Option<char> {
    let (marker, _) = split_first_char(trimmed)?;
    (matches!(marker, '=' | '-' | '~' | '^' | '+') && trimmed.chars().all(|ch| ch == marker))
        .then_some(marker)
}

fn is_setext_title_text(trimmed: &str) -> bool {
    !(trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with(':')
        || is_block_attribute_line(trimmed)
        || is_atx_title(trimmed)
        || root_list_family(trimmed).is_some()
        || delimiter_token(trimmed).is_some())
}

#[derive(Default)]
struct HeaderLines {
    title: Option<usize>,
    author: Option<usize>,
    revision: Option<usize>,
}

fn collect_header_lines(lines: &[SourceLine<'_>]) -> HeaderLines {
    let mut header = HeaderLines::default();
    let mut iter = lines.iter().peekable();

    while let Some(line) = iter.next() {
        let trimmed = line.text.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || is_block_attribute_line(trimmed) {
            continue;
        }
        if is_atx_title(trimmed) {
            header.title = Some(line.number);
            break;
        }
        if let Some(next) = iter.peek()
            && is_setext_underline(next.text.trim())
        {
            header.title = Some(line.number);
            break;
        }
        break;
    }

    let Some(title_line) = header.title else {
        return header;
    };

    let mut after_author = false;
    for line in lines.iter().filter(|line| line.number > title_line) {
        let trimmed = line.text.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.is_empty() {
            if after_author {
                break;
            }
            continue;
        }
        if !after_author {
            if trimmed.starts_with(':') {
                break;
            }
            header.author = Some(line.number);
            after_author = true;
            continue;
        }
        if looks_like_revision_line(trimmed) {
            header.revision = Some(line.number);
        }
        break;
    }

    header
}

fn looks_like_revision_line(trimmed: &str) -> bool {
    if is_date_like(trimmed) {
        return true;
    }
    let candidate = trimmed.strip_prefix('v').unwrap_or(trimmed);
    candidate
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
        && candidate.contains('.')
}

fn is_date_like(value: &str) -> bool {
    let mut parts = value.split('-');
    let year = parts.next();
    let month = parts.next();
    let day = parts.next();
    parts.next().is_none()
        && year.is_some_and(|part| {
            part.chars().count() == 4 && part.chars().all(|ch| ch.is_ascii_digit())
        })
        && month.is_some_and(|part| {
            part.chars().count() == 2 && part.chars().all(|ch| ch.is_ascii_digit())
        })
        && day.is_some_and(|part| {
            part.chars().count() == 2 && part.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn line_after(line: u32) -> usize {
    usize::try_from(line)
        .unwrap_or(usize::MAX)
        .saturating_add(1)
}

#[cfg(test)]
mod tests {
    use crate::{
        Error, LintId, LintLevel, LintOptions, LintOverride, LintSelector, LintSourcePosition,
        LintSourceRange, Lintable,
    };

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn section_title_symmetric_marker_flags_symmetric_titles() -> Result<(), Error> {
        let report = report_for("= Title\n\n== Section ==\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::SectionTitleSymmetricMarker));
        Ok(())
    }

    #[test]
    fn section_title_symmetric_marker_ignores_text_inside_listing() -> Result<(), Error> {
        let report = report_for("= Title\n\n----\n== Not Section ==\n----\n")?;

        assert!(!has_lint(&report, LintId::SectionTitleSymmetricMarker));
        Ok(())
    }

    #[test]
    fn section_title_setext_style_flags_setext_title_pairs() -> Result<(), Error> {
        let report = report_for("= Title\n\nSetext\n^^^^^^\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::SectionTitleSetextStyle));
        Ok(())
    }

    #[test]
    fn section_title_marker_spacing_flags_missing_space() -> Result<(), Error> {
        let report = report_for("= Title\n\n==Section\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::SectionTitleMarkerSpacing));
        Ok(())
    }

    #[test]
    fn section_title_capitalization_flags_lowercase_titles() -> Result<(), Error> {
        let report = report_for("= title\n\n== section\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::SectionTitleCapitalization));
        Ok(())
    }

    #[test]
    fn section_title_capitalization_ignores_leading_monospace_titles() -> Result<(), Error> {
        let report = report_for("= `acdc-lint`\n\n== `acdc-cli`\n\nContent.\n")?;

        assert!(!has_lint(&report, LintId::SectionTitleCapitalization));
        assert!(!has_lint(
            &report,
            LintId::SectionTitleCapitalizationMonospace
        ));
        Ok(())
    }

    #[test]
    fn section_title_capitalization_flags_lowercase_bold_titles() -> Result<(), Error> {
        let report = report_for("= **a tool**\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::SectionTitleCapitalization));
        assert!(!has_lint(
            &report,
            LintId::SectionTitleCapitalizationMonospace
        ));
        Ok(())
    }

    #[test]
    fn section_title_capitalization_monospace_is_opt_in() -> Result<(), Error> {
        let source = "= `acdc-lint`\n\nContent.\n";
        let options = LintOptions::new(vec![LintOverride::new(
            LintLevel::Warn,
            LintSelector::Lint(LintId::SectionTitleCapitalizationMonospace),
        )]);
        let report = source.lint(&options)?;

        assert!(!has_lint(&report, LintId::SectionTitleCapitalization));
        assert!(has_lint(
            &report,
            LintId::SectionTitleCapitalizationMonospace
        ));
        Ok(())
    }

    #[test]
    fn location_scoped_override_only_suppresses_matching_title() -> Result<(), Error> {
        let source = "= title\n\n== section\n\nContent.\n";
        let options = LintOptions::new(vec![LintOverride::with_location(
            LintLevel::Allow,
            LintSelector::Lint(LintId::SectionTitleCapitalization),
            LintSourceRange::point(LintSourcePosition::new(1, None)),
        )]);
        let report = source.lint(&options)?;
        let diagnostics = report
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.lint() == LintId::SectionTitleCapitalization)
            .collect::<Vec<_>>();

        assert_eq!(diagnostics.len(), 1);
        let diagnostic = diagnostics.first();
        assert_eq!(
            diagnostic
                .and_then(|diagnostic| diagnostic.location())
                .map(|location| location.location.start.line),
            Some(3)
        );
        Ok(())
    }

    #[test]
    fn location_scoped_override_reports_stale_location() -> Result<(), Error> {
        let source = "= Title\n\nContent.\n";
        let options = LintOptions::new(vec![LintOverride::with_location(
            LintLevel::Allow,
            LintSelector::Lint(LintId::SectionTitleCapitalization),
            LintSourceRange::point(LintSourcePosition::new(1, None)),
        )]);
        let report = source.lint(&options)?;

        assert!(report.diagnostics().iter().any(|diagnostic| {
            diagnostic.lint() == LintId::SectionTitleCapitalization
                && diagnostic
                    .message()
                    .contains("did not match any diagnostic")
        }));
        Ok(())
    }

    #[test]
    fn document_header_lints_are_opt_in() -> Result<(), Error> {
        let source = "= Title\n\nContent.\n";
        let report = report_for(source)?;
        assert!(!has_lint(&report, LintId::DocumentTitleAuthor));

        let options = LintOptions::new(vec![LintOverride::new(
            LintLevel::Warn,
            LintSelector::Lint(LintId::DocumentTitleAuthor),
        )]);
        let report = source.lint(&options)?;
        assert!(has_lint(&report, LintId::DocumentTitleAuthor));

        Ok(())
    }

    #[test]
    fn document_title_revision_is_opt_in() -> Result<(), Error> {
        let source = "= Title\nAuthor Name\n\nContent.\n";
        let options = LintOptions::new(vec![LintOverride::new(
            LintLevel::Warn,
            LintSelector::Lint(LintId::DocumentTitleRevision),
        )]);
        let report = source.lint(&options)?;

        assert!(has_lint(&report, LintId::DocumentTitleRevision));
        Ok(())
    }
}
