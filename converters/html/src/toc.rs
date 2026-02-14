use std::io::Write;

use acdc_converters_core::{toc::Config as TocConfig, visitor::WritableVisitor};
use acdc_parser::{AttributeValue, MAX_SECTION_LEVELS, MAX_TOC_LEVELS, TableOfContents, TocEntry};

use acdc_converters_core::section::{DEFAULT_SECTION_LEVEL, to_upper_roman};

use crate::{Error, HtmlVariant, HtmlVisitor, Processor};

struct SectionNumberConfig<'a> {
    sectnums_enabled: bool,
    sectnumlevels: u8,
    partnums_enabled: bool,
    part_signifier: Option<&'a str>,
    appendix_caption: Option<&'a str>,
}

struct TocRenderConfig<'a> {
    max_level: u8,
    section_numbers: &'a [Option<String>],
    semantic: bool,
}

/// Returns the effective TOC level for an entry.
/// Appendix level-0 entries are demoted to level 1.
fn effective_toc_level(entry: &TocEntry) -> u8 {
    if entry.level == 0 && entry.style.as_ref().is_some_and(|s| s == "appendix") {
        1
    } else {
        entry.level
    }
}

/// Compute section numbers for TOC entries.
/// Returns a vector of optional section number strings for each entry.
fn compute_toc_section_numbers(
    entries: &[TocEntry],
    config: &SectionNumberConfig,
) -> Vec<Option<String>> {
    if !config.sectnums_enabled && !config.partnums_enabled && config.appendix_caption.is_none() {
        return vec![None; entries.len()];
    }

    let mut counters = [0u8; MAX_TOC_LEVELS as usize + 1];
    let mut part_counter: usize = 0;
    let mut appendix_counter: usize = 0;
    let mut numbers = Vec::with_capacity(entries.len());

    for entry in entries {
        let level = entry.level;
        let is_appendix = entry.style.as_ref().is_some_and(|s| s == "appendix");

        // Appendix sections: use letter numbering (A, B, C) instead of regular numbering.
        // Check before the `!numbered` skip since appendix is in UNNUMBERED_SECTION_STYLES.
        if is_appendix {
            counters.fill(0);
            if let Some(caption) = config.appendix_caption {
                let letter =
                    char::from(b'A' + u8::try_from(appendix_counter).unwrap_or(25).min(25));
                appendix_counter += 1;
                numbers.push(Some(format!("{caption} {letter}: ")));
            } else {
                appendix_counter += 1;
                numbers.push(None);
            }
            continue;
        }

        // Level 0 (parts): number with Roman numerals if :partnums: is set
        if level == 0 {
            if config.partnums_enabled {
                part_counter += 1;
                // Reset section counters at part boundary
                counters.fill(0);
                let roman = to_upper_roman(part_counter);
                let formatted = if let Some(sig) = config.part_signifier {
                    format!("{sig} {roman}: ")
                } else {
                    format!("{roman}: ")
                };
                numbers.push(Some(formatted));
            } else {
                numbers.push(None);
            }
            continue;
        }

        // Skip numbering for special sections (bibliography, glossary, etc.)
        // Don't increment counters — subsequent sections continue the sequence.
        // Checked after appendix/part handling so those get their own labels.
        if !entry.numbered {
            numbers.push(None);
            continue;
        }

        if level > MAX_TOC_LEVELS + 1 {
            numbers.push(None);
            continue;
        }

        if !config.sectnums_enabled {
            numbers.push(None);
            continue;
        }

        let level_idx = (level - 1) as usize;

        // Increment counter for this level (safe: level is 1-6, so level_idx is 0-5)
        if let Some(counter) = counters.get_mut(level_idx) {
            *counter += 1;
        } else {
            numbers.push(None);
            continue;
        }

        // Reset deeper levels
        for counter in counters.iter_mut().skip(level_idx + 1) {
            *counter = 0;
        }

        // Only show number if within sectnumlevels
        if level <= config.sectnumlevels {
            if let Some(slice) = counters.get(..=level_idx) {
                let number: String = slice
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(".");
                numbers.push(Some(format!("{number}. ")));
            } else {
                numbers.push(None);
            }
        } else {
            numbers.push(None);
        }
    }

    numbers
}

/// Render TOC entries recursively.
///
/// When `parts_at_current_level` is true, level-0 entries (parts) are rendered
/// alongside level-1 entries in the same list. This matches asciidoctor behavior
/// when pre-part sections exist before the first level-0 section.
fn render_entries<W: Write>(
    entries: &[TocEntry],
    visitor: &mut HtmlVisitor<W>,
    config: &TocRenderConfig,
    current_level: u8,
    base_index: usize,
    parts_at_current_level: bool,
) -> Result<(), Error> {
    use acdc_converters_core::visitor::Visitor;

    if current_level > config.max_level {
        return Ok(());
    }

    // When parts_at_current_level is true, include level-0 entries alongside
    // level-1 entries. Only include level-1 entries that appear before the
    // first level-0 entry (pre-part sections); level-1 entries after a part
    // are children of that part.
    // Note: appendix level-0 entries are treated as level 1 via effective_toc_level.
    let first_real_part_idx = if parts_at_current_level {
        entries
            .iter()
            .position(|e| e.level == 0 && effective_toc_level(e) == 0)
    } else {
        None
    };

    let current_level_entries: Vec<(usize, &TocEntry)> = entries
        .iter()
        .enumerate()
        .filter(|(idx, entry)| {
            let eff_level = effective_toc_level(entry);
            if eff_level == current_level {
                // When merging, only include level-1 entries before the first part
                if let Some(first_l0) = first_real_part_idx {
                    *idx < first_l0 || entry.level != current_level
                } else {
                    true
                }
            } else {
                // Include level-0 entries (non-appendix) at the level-1 tier
                parts_at_current_level && entry.level == 0 && eff_level == 0
            }
        })
        .collect();

    if current_level_entries.is_empty() {
        return Ok(());
    }

    if config.semantic {
        writeln!(
            visitor.writer_mut(),
            "<ol class=\"toc-list level-{current_level}\">"
        )?;
    } else {
        writeln!(
            visitor.writer_mut(),
            "<ul class=\"sectlevel{current_level}\">"
        )?;
    }

    for (i, (entry_index, entry)) in current_level_entries.iter().enumerate() {
        writeln!(visitor.writer_mut(), "<li>")?;
        write!(visitor.writer_mut(), "<a href=\"#{}\">", entry.id)?;

        // Include section number if available
        let global_index = base_index + entry_index;
        if let Some(Some(number)) = config.section_numbers.get(global_index) {
            write!(visitor.writer_mut(), "{number}")?;
        }

        // Enable TOC mode to render inline nodes without nested links
        let was_toc_mode = visitor.render_options.toc_mode;
        visitor.render_options.toc_mode = true;
        visitor.visit_inline_nodes(&entry.title)?;
        visitor.render_options.toc_mode = was_toc_mode;

        writeln!(visitor.writer_mut(), "</a>")?;
        // Find children: entries that come after this one but before the next
        // entry at the current tier
        let start_search = entry_index + 1;
        let end_search = if let Some(next_entry) = current_level_entries.get(i + 1) {
            next_entry.0 // Next entry at current level
        } else {
            entries.len() // End of all entries
        };

        // Detect direct children using the entry's effective level:
        // - For level-0 entries (parts): children are at level 1
        // - For appendix entries (demoted to 1): children are at level 2
        // - For level-N entries: children are at level N+1
        let child_level = effective_toc_level(entry) + 1;

        if let Some(direct_children) = entries.get(start_search..end_search) {
            let has_children = direct_children.iter().any(|e| e.level == child_level);

            if has_children && child_level <= config.max_level {
                render_entries(
                    direct_children,
                    visitor,
                    config,
                    child_level,
                    base_index + start_search,
                    false, // no more merging in nested lists
                )?;
            }
        }
        writeln!(visitor.writer_mut(), "</li>")?;
    }

    if config.semantic {
        writeln!(visitor.writer_mut(), "</ol>")?;
    } else {
        writeln!(visitor.writer_mut(), "</ul>")?;
    }
    Ok(())
}

fn section_number_config(processor: &Processor) -> SectionNumberConfig<'_> {
    // Also check :numbered: as a deprecated alias for :sectnums:
    let sectnums_enabled = processor
        .document_attributes()
        .get("sectnums")
        .or_else(|| processor.document_attributes().get("numbered"))
        .is_some_and(|v| !matches!(v, AttributeValue::Bool(false)));
    // Clamp to valid range: 0-5 (0 effectively disables numbering)
    let sectnumlevels = processor
        .document_attributes()
        .get_string("sectnumlevels")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SECTION_LEVEL)
        .min(MAX_SECTION_LEVELS);
    // Appendix caption: default "Appendix", customizable via :appendix-caption:,
    // disabled if :appendix-caption!: is set
    let appendix_caption = match processor.document_attributes().get("appendix-caption") {
        Some(AttributeValue::String(s)) => Some(s.as_str()),
        Some(AttributeValue::Bool(false)) => None,
        _ => Some("Appendix"),
    };
    SectionNumberConfig {
        sectnums_enabled,
        sectnumlevels,
        partnums_enabled: processor.part_number_tracker().is_enabled(),
        part_signifier: processor.part_number_tracker().signifier(),
        appendix_caption,
    }
}

pub(crate) fn render<W: Write>(
    toc_macro: Option<&TableOfContents>,
    visitor: &mut HtmlVisitor<W>,
    placement: &str,
    processor: &Processor,
) -> Result<(), Error> {
    let config = TocConfig::from_attributes(toc_macro, &processor.document_attributes);

    // Determine if TOC should render at this placement point
    // - "auto" placement point accepts: auto, left, right, top, bottom (all render in header)
    // - "preamble" placement point accepts: preamble
    // - "macro" placement point accepts: macro
    let should_render = match placement {
        "auto" => matches!(
            config.placement(),
            "auto" | "left" | "right" | "top" | "bottom"
        ),
        other => config.placement() == other,
    };

    if should_render && !processor.toc_entries.is_empty() {
        let semantic = processor.variant() == HtmlVariant::Semantic;
        let embedded = visitor.render_options.embedded;

        // In embedded mode, sidebar positioning doesn't apply, so downgrade toc2 → toc
        let toc_class = if embedded && config.toc_class() == "toc2" {
            "toc"
        } else {
            config.toc_class()
        };

        // toc::[] macro adds class="title" to the toctitle div
        let is_macro = placement == "macro";

        let section_numbers =
            compute_toc_section_numbers(&processor.toc_entries, &section_number_config(processor));

        if semantic {
            writeln!(
                visitor.writer_mut(),
                "<nav id=\"toc\" class=\"{toc_class}\" role=\"doc-toc\">"
            )?;
            let title = config.title().unwrap_or("Table of Contents");
            writeln!(visitor.writer_mut(), "<h2 id=\"toc-title\">{title}</h2>")?;
        } else {
            writeln!(
                visitor.writer_mut(),
                "<div id=\"toc\" class=\"{toc_class}\">"
            )?;
            let title_class = if is_macro { " class=\"title\"" } else { "" };
            if let Some(title) = config.title() {
                writeln!(
                    visitor.writer_mut(),
                    "<div id=\"toctitle\"{title_class}>{title}</div>"
                )?;
            } else {
                writeln!(
                    visitor.writer_mut(),
                    "<div id=\"toctitle\"{title_class}>Table of Contents</div>"
                )?;
            }
        }

        // Determine starting level: use the first entry's effective level.
        // Appendix level-0 entries are demoted to level 1, so they don't count as parts.
        // When pre-part sections (level 1) appear before the first part (level 0),
        // the outer list starts at sectlevel1 and parts are merged into that tier.
        let first_level = processor.toc_entries.first().map_or(1, effective_toc_level);
        let has_real_parts = processor
            .toc_entries
            .iter()
            .any(|e| e.level == 0 && effective_toc_level(e) == 0);
        let parts_at_current_level = first_level > 0 && has_real_parts;
        let start_level = if parts_at_current_level {
            1
        } else {
            first_level
        };

        let render_config = TocRenderConfig {
            max_level: config.levels(),
            section_numbers: &section_numbers,
            semantic,
        };
        render_entries(
            &processor.toc_entries,
            visitor,
            &render_config,
            start_level,
            0,
            parts_at_current_level,
        )?;
        if semantic {
            writeln!(visitor.writer_mut(), "</nav>")?;
        } else {
            writeln!(visitor.writer_mut(), "</div>")?;
        }
    }
    Ok(())
}
