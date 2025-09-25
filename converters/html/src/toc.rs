use std::io::Write;

use acdc_parser::{AttributeValue, DocumentAttributes, TableOfContents, TocEntry};

use crate::{Processor, Render, RenderOptions};

pub fn render_html<W: Write>(
    toc_entries: &[TocEntry],
    w: &mut W,
    processor: &Processor,
    options: &RenderOptions,
    max_level: u8,
    title: &str,
) -> Result<(), crate::Error> {
    writeln!(w, "<div id=\"toc\" class=\"toc\">")?;
    writeln!(w, "<div id=\"toctitle\">{title}</div>")?;

    if !toc_entries.is_empty() {
        render_list(toc_entries, w, processor, options, max_level, 1)?;
    }

    writeln!(w, "</div>")?;
    Ok(())
}

fn render_list<W: Write>(
    entries: &[TocEntry],
    w: &mut W,
    processor: &Processor,
    options: &RenderOptions,
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

    writeln!(w, "<ul class=\"sectlevel{current_level}\">")?;

    for (i, (entry_index, entry)) in current_level_entries.iter().enumerate() {
        writeln!(w, "<li>")?;
        write!(w, "<a href=\"#{}\">", entry.id,)?;
        crate::inlines::render_inlines(&entry.title, w, processor, options)?;
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
            render_list(
                child_slice,
                w,
                processor,
                options,
                max_level,
                current_level + 1,
            )?;
        }

        writeln!(w, "</li>")?;
    }

    writeln!(w, "</ul>")?;
    Ok(())
}

pub(crate) fn get_placement_from_attributes(attributes: &DocumentAttributes) -> &str {
    attributes.get("toc").map_or("auto", |v| match v {
        AttributeValue::String(s) => s.as_str(),
        AttributeValue::Bool(true) => "auto",
        _ => "none",
    })
}

pub(crate) fn render<W: Write>(
    w: &mut W,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), crate::Error> {
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

        render_html(
            &processor.toc_entries,
            w,
            processor,
            options,
            toc_levels,
            toc_title,
        )?;
    }

    Ok(())
}

impl Render for TableOfContents {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        let toc_placement = get_placement_from_attributes(&processor.document_attributes);
        if toc_placement == "macro" {
            render(w, processor, options)?;
        }
        Ok(())
    }
}
