use std::io::Write;

use acdc_converters_core::{toc::Config as TocConfig, visitor::WritableVisitor};
use acdc_parser::{AttributeValue, MAX_SECTION_LEVELS, MAX_TOC_LEVELS, TableOfContents, TocEntry};

use crate::{
    Error, HtmlVariant, HtmlVisitor, Processor,
    section::{DEFAULT_SECTION_LEVEL, to_upper_roman},
};

/// Compute section numbers for TOC entries.
/// Returns a vector of optional section number strings for each entry.
fn compute_toc_section_numbers(
    entries: &[TocEntry],
    sectnums_enabled: bool,
    sectnumlevels: u8,
    partnums_enabled: bool,
    part_signifier: Option<&str>,
) -> Vec<Option<String>> {
    if !sectnums_enabled && !partnums_enabled {
        return vec![None; entries.len()];
    }

    let mut counters = [0u8; MAX_TOC_LEVELS as usize + 1];
    let mut part_counter: usize = 0;
    let mut numbers = Vec::with_capacity(entries.len());

    for entry in entries {
        let level = entry.level;

        // Skip numbering for special sections (bibliography, glossary, etc.)
        // Don't increment counters - subsequent sections continue the sequence
        if !entry.numbered {
            numbers.push(None);
            continue;
        }

        if level > MAX_TOC_LEVELS + 1 {
            numbers.push(None);
            continue;
        }

        // Level 0 (parts): number with Roman numerals if :partnums: is set
        if level == 0 {
            if partnums_enabled {
                part_counter += 1;
                // Reset section counters at part boundary
                counters.fill(0);
                let roman = to_upper_roman(part_counter);
                let formatted = if let Some(sig) = part_signifier {
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

        if !sectnums_enabled {
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
        if level <= sectnumlevels {
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
#[allow(clippy::too_many_arguments)]
fn render_entries<W: Write>(
    entries: &[TocEntry],
    visitor: &mut HtmlVisitor<W>,
    max_level: u8,
    current_level: u8,
    section_numbers: &[Option<String>],
    base_index: usize,
    semantic: bool,
    parts_at_current_level: bool,
) -> Result<(), Error> {
    use acdc_converters_core::visitor::Visitor;

    if current_level > max_level {
        return Ok(());
    }

    // When parts_at_current_level is true, include level-0 entries alongside
    // level-1 entries. Only include level-1 entries that appear before the
    // first level-0 entry (pre-part sections); level-1 entries after a part
    // are children of that part.
    let first_level0_idx = if parts_at_current_level {
        entries.iter().position(|e| e.level == 0)
    } else {
        None
    };

    let current_level_entries: Vec<(usize, &TocEntry)> = entries
        .iter()
        .enumerate()
        .filter(|(idx, entry)| {
            if entry.level == current_level {
                // When merging, only include level-1 entries before the first part
                if let Some(first_l0) = first_level0_idx {
                    *idx < first_l0
                } else {
                    true
                }
            } else {
                // Include level-0 entries at the level-1 tier
                parts_at_current_level && entry.level == 0
            }
        })
        .collect();

    if current_level_entries.is_empty() {
        return Ok(());
    }

    if semantic {
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
        if let Some(Some(number)) = section_numbers.get(global_index) {
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

        // Detect direct children using the entry's actual level:
        // - For level-0 entries (parts): children are at level 1
        // - For level-N entries: children are at level N+1
        let child_level = entry.level + 1;

        if let Some(direct_children) = entries.get(start_search..end_search) {
            let has_children = direct_children.iter().any(|e| e.level == child_level);

            if has_children && child_level <= max_level {
                render_entries(
                    direct_children,
                    visitor,
                    max_level,
                    child_level,
                    section_numbers,
                    base_index + start_search,
                    semantic,
                    false, // no more merging in nested lists
                )?;
            }
        }
        writeln!(visitor.writer_mut(), "</li>")?;
    }

    if semantic {
        writeln!(visitor.writer_mut(), "</ol>")?;
    } else {
        writeln!(visitor.writer_mut(), "</ul>")?;
    }
    Ok(())
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

        // Compute section numbers for TOC entries
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
        let partnums_enabled = processor.part_number_tracker().is_enabled();
        let part_signifier = processor.part_number_tracker().signifier();
        let section_numbers = compute_toc_section_numbers(
            &processor.toc_entries,
            sectnums_enabled,
            sectnumlevels,
            partnums_enabled,
            part_signifier,
        );

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

        // Determine starting level: use the first entry's level.
        // When pre-part sections (level 1) appear before the first part (level 0),
        // the outer list starts at sectlevel1 and parts are merged into that tier.
        let first_level = processor.toc_entries.first().map_or(1, |e| e.level);
        let has_parts = processor.toc_entries.iter().any(|e| e.level == 0);
        let parts_at_current_level = first_level > 0 && has_parts;
        let start_level = if parts_at_current_level {
            1
        } else {
            first_level
        };

        render_entries(
            &processor.toc_entries,
            visitor,
            config.levels(),
            start_level,
            &section_numbers,
            0,
            semantic,
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
