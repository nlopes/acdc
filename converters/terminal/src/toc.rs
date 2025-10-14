use std::io::Write;

use acdc_converters_common::toc::get_placement_from_attributes;
use acdc_parser::{AttributeValue, TableOfContents, TocEntry};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Processor, Render};

fn render_list<W: Write>(
    entries: &[acdc_parser::TocEntry],
    w: &mut W,
    processor: &Processor,
    max_level: u8,
    current_level: u8,
) -> Result<(), crate::Error> {
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

    for (i, (entry_index, entry)) in current_level_entries.iter().enumerate() {
        write!(w, "{:indent$}", "", indent = current_level as usize - 1)?;
        for inline in &entry.title {
            inline.render(w, processor)?;
        }
        writeln!(w)?;

        // Find children: entries that come after this one and have level = current_level + 1
        // but before the next entry at current_level or lower
        let start_search = entry_index + 1;
        let end_search = if i + 1 < current_level_entries.len() {
            current_level_entries[i + 1].0 // Next entry at current level
        } else {
            entries.len() // End of all entries
        };

        // Find children: only entries that are direct children (level = current_level + 1)
        // and stop when we hit another entry at current_level or higher
        let mut children: Vec<&TocEntry> = Vec::new();
        for entry in &entries[start_search..end_search] {
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
            let child_slice = &entries[start_search..end_search];
            render_list(child_slice, w, processor, max_level, current_level + 1)?;
        }
    }
    Ok(())
}

pub(crate) fn render<W: Write>(w: &mut W, processor: &Processor) -> Result<(), crate::Error> {
    // Use parser-collected TOC entries
    if !processor.toc_entries.is_empty() {
        // Get TOC configuration
        let toc_title = processor
            .document_attributes
            .get("toc-title")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or("Table of Contents");

        let toc_levels = processor
            .document_attributes
            .get("toclevels")
            .and_then(|v| match v {
                AttributeValue::String(s) => s.parse::<u8>().ok(),
                _ => None,
            })
            .unwrap_or(2);

        w.queue(PrintStyledContent(toc_title.bold()))?;
        writeln!(w)?;

        render_list(&processor.toc_entries, w, processor, toc_levels, 1)?;
        writeln!(w)?;
    }

    Ok(())
}

impl Render for TableOfContents {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        let toc_placement = get_placement_from_attributes(&processor.document_attributes);
        if toc_placement == "macro" {
            render(w, processor)?;
        }
        Ok(())
    }
}
