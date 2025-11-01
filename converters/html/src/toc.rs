use acdc_converters_common::{toc::Config as TocConfig, visitor::WritableVisitor};
use acdc_parser::{TableOfContents, TocEntry};

use crate::{Error, Processor};

fn render_entries<V: WritableVisitor<Error = Error>>(
    entries: &[TocEntry],
    visitor: &mut V,
    max_level: u8,
    current_level: u8,
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
        write!(w, "<a href=\"#{}\">", entry.id,)?;
        let _ = w;
        visitor.visit_inline_nodes(&entry.title)?;
        w = visitor.writer_mut();
        writeln!(w, "</a>")?;
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
            // Create a slice containing potential children and their descendants
            let child_slice = &entries[start_search..end_search];
            let _ = w;
            render_entries(child_slice, visitor, max_level, current_level + 1)?;
            w = visitor.writer_mut();
        }

        writeln!(w, "</li>")?;
    }

    writeln!(w, "</ul>")?;
    Ok(())
}

pub(crate) fn render<V: WritableVisitor<Error = Error>>(
    _toc_macro: Option<&TableOfContents>,
    visitor: &mut V,
    placement: &str,
    processor: &Processor,
) -> Result<(), Error> {
    let config = TocConfig::from(&processor.document_attributes);
    if config.placement == placement && !processor.toc_entries.is_empty() {
        let w = visitor.writer_mut();
        writeln!(w, "<div id=\"toc\" class=\"toc\">")?;
        if let Some(title) = &config.title {
            writeln!(w, "<div id=\"toctitle\">{title}</div>")?;
        } else {
            writeln!(w, "<div id=\"toctitle\">Table of Contents</div>")?;
        }
        render_entries(&processor.toc_entries, visitor, config.levels, 1)?;
        let w = visitor.writer_mut();
        writeln!(w, "</div>")?;
    }
    Ok(())
}
