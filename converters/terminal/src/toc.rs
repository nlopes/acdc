use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{TableOfContents, TocEntry};

use crate::Processor;

fn render_entries<V: WritableVisitor<Error = crate::Error>>(
    entries: &[TocEntry],
    visitor: &mut V,
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
        let mut w = visitor.writer_mut();
        write!(w, "{:indent$}", "", indent = current_level as usize - 1)?;
        let _ = w;
        for inline in &entry.title {
            visitor.visit_inline_node(inline)?;
        }
        w = visitor.writer_mut();
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
            render_entries(child_slice, visitor, max_level, current_level + 1)?;
        }
    }
    Ok(())
}

pub(crate) fn render<V: WritableVisitor<Error = crate::Error>>(
    toc_macro: Option<&TableOfContents>,
    visitor: &mut V,
    placement: &str,
    processor: &Processor,
) -> Result<(), crate::Error> {
    use acdc_converters_common::toc::Config as TocConfig;
    use crossterm::{
        QueueableCommand,
        style::{PrintStyledContent, Stylize},
    };

    let config = TocConfig::from_attributes(toc_macro, &processor.document_attributes);
    if config.placement == placement && !processor.toc_entries.is_empty() {
        let w = visitor.writer_mut();
        if let Some(title) = config.title {
            w.queue(PrintStyledContent(title.bold()))?;
        } else {
            w.queue(PrintStyledContent("Table of Contents".bold()))?;
        }
        writeln!(w)?;
        render_entries(&processor.toc_entries, visitor, config.levels, 1)?;
        let w = visitor.writer_mut();
        writeln!(w)?;
    }
    Ok(())
}
