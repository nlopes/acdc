use acdc_converters_core::{toc::Config as TocConfig, visitor::WritableVisitor};
use acdc_parser::{AttributeValue, MAX_SECTION_LEVELS, MAX_TOC_LEVELS, TableOfContents, TocEntry};

use crate::{Error, Processor, section::DEFAULT_SECTION_LEVEL};

/// Compute section numbers for TOC entries.
/// Returns a vector of optional section number strings for each entry.
fn compute_toc_section_numbers(
    entries: &[TocEntry],
    sectnums_enabled: bool,
    sectnumlevels: u8,
) -> Vec<Option<String>> {
    if !sectnums_enabled {
        return vec![None; entries.len()];
    }

    let mut counters = [0u8; MAX_TOC_LEVELS as usize + 1];
    let mut numbers = Vec::with_capacity(entries.len());

    for entry in entries {
        let level = entry.level;

        // Skip numbering for special sections (bibliography, glossary, etc.)
        // Don't increment counters - subsequent sections continue the sequence
        if !entry.numbered {
            numbers.push(None);
            continue;
        }

        if level == 0 || level > MAX_TOC_LEVELS + 1 {
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

fn render_entries<V: WritableVisitor<Error = Error>>(
    entries: &[TocEntry],
    visitor: &mut V,
    max_level: u8,
    current_level: u8,
    section_numbers: &[Option<String>],
    base_index: usize,
) -> Result<(), Error> {
    if current_level > max_level {
        return Ok(());
    }

    // Filter entries to only process those at the current level
    let current_level_entries: Vec<(usize, &TocEntry)> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.level == current_level)
        .collect();

    if current_level_entries.is_empty() {
        return Ok(());
    }

    let mut w = visitor.writer_mut();
    writeln!(w, "<ul class=\"sectlevel{current_level}\">")?;

    for (i, (entry_index, entry)) in current_level_entries.iter().enumerate() {
        writeln!(w, "<li>")?;
        write!(w, "<a href=\"#{}\">", entry.id)?;

        // Include section number if available
        let global_index = base_index + entry_index;
        if let Some(Some(number)) = section_numbers.get(global_index) {
            write!(w, "{number}")?;
        }

        let _ = w;
        visitor.visit_inline_nodes(&entry.title)?;
        w = visitor.writer_mut();
        writeln!(w, "</a>")?;
        // Find children: entries that come after this one and have level = current_level + 1
        // but before the next entry at current_level or lower
        let start_search = entry_index + 1;
        let end_search = if let Some(next_entry) = current_level_entries.get(i + 1) {
            next_entry.0 // Next entry at current level
        } else {
            entries.len() // End of all entries
        };

        // Find children: only entries that are direct children (level = current_level + 1)
        // and stop when we hit another entry at current_level or higher
        if let Some(direct_children) = entries.get(start_search..end_search) {
            let mut children: Vec<&TocEntry> = Vec::new();
            for entry in direct_children {
                // Stop if we encounter another entry at the same level or higher
                // This prevents us from claiming children that belong to later siblings
                if entry.level <= current_level {
                    break;
                }
                if entry.level == current_level + 1 {
                    children.push(entry);
                }
            }

            if !children.is_empty() && current_level < max_level {
                // Create a slice containing potential children and their descendants
                let _ = w;
                render_entries(
                    direct_children,
                    visitor,
                    max_level,
                    current_level + 1,
                    section_numbers,
                    base_index + start_search,
                )?;
                w = visitor.writer_mut();
            }
        }
        writeln!(w, "</li>")?;
    }

    writeln!(w, "</ul>")?;
    Ok(())
}

pub(crate) fn render<V: WritableVisitor<Error = Error>>(
    toc_macro: Option<&TableOfContents>,
    visitor: &mut V,
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
        let section_numbers =
            compute_toc_section_numbers(&processor.toc_entries, sectnums_enabled, sectnumlevels);

        let w = visitor.writer_mut();
        writeln!(w, "<div id=\"toc\" class=\"{}\">", config.toc_class())?;
        if let Some(title) = config.title() {
            writeln!(w, "<div id=\"toctitle\">{title}</div>")?;
        } else {
            writeln!(w, "<div id=\"toctitle\">Table of Contents</div>")?;
        }
        render_entries(
            &processor.toc_entries,
            visitor,
            config.levels(),
            1,
            &section_numbers,
            0,
        )?;
        let w = visitor.writer_mut();
        writeln!(w, "</div>")?;
    }
    Ok(())
}
