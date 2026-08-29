use std::io::Write;

use acdc_converters_core::{
    toc::{Config as TocConfig, NumberingConfig, effective_level, has_real_parts, section_numbers},
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{AttributeValue, SectionKind, TableOfContents, TocEntry};

use crate::TerminalVisitor;

struct TocRenderConfig<'a> {
    max_level: u8,
    section_numbers: &'a [Option<String>],
    has_real_parts: bool,
}

impl<W: Write> TerminalVisitor<'_, '_, W> {
    #[allow(clippy::too_many_arguments)]
    fn render_toc_entries(
        &mut self,
        entries: &[TocEntry],
        config: &TocRenderConfig<'_>,
        current_level: u8,
        base_index: usize,
        parts_at_current_level: bool,
        indent: usize,
    ) -> Result<(), crate::Error> {
        if current_level > config.max_level {
            return Ok(());
        }

        let first_real_part = if parts_at_current_level {
            entries
                .iter()
                .position(|entry| entry.level == 0 && entry.kind == SectionKind::Normal)
        } else {
            None
        };
        let current_entries: Vec<(usize, &TocEntry)> = entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| {
                let level = effective_level(entry, config.has_real_parts);
                if level == current_level {
                    first_real_part.is_none_or(|part| *index < part || entry.level != current_level)
                } else {
                    parts_at_current_level && entry.level == 0 && level == 0
                }
            })
            .collect();

        for (position, (entry_index, entry)) in current_entries.iter().enumerate() {
            write!(self.writer, "{:indent$}", "", indent = indent)?;
            if let Some(Some(number)) = config.section_numbers.get(base_index + entry_index) {
                write!(self.writer, "{number}")?;
            }
            self.visit_inline_nodes(&entry.title)?;
            writeln!(self.writer)?;

            let start = entry_index + 1;
            let end = current_entries
                .get(position + 1)
                .map_or(entries.len(), |next| next.0);
            let child_level = if entry.level == 0 && entry.kind.is_special() {
                2
            } else {
                effective_level(entry, config.has_real_parts) + 1
            };
            if let Some(children) = entries.get(start..end)
                && child_level <= config.max_level
                && children.iter().any(|child| child.level == child_level)
            {
                self.render_toc_entries(
                    children,
                    config,
                    child_level,
                    base_index + start,
                    false,
                    indent + 2,
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn render_toc(
        &mut self,
        toc_macro: Option<&TableOfContents>,
        placement: &str,
    ) -> Result<(), crate::Error> {
        use crossterm::{
            QueueableCommand,
            style::{PrintStyledContent, Stylize},
        };

        let processor = self.processor.clone();
        let config = TocConfig::from_attributes(toc_macro, &processor.document_attributes);
        let should_render = match placement {
            "auto" => matches!(
                config.placement(),
                "auto" | "left" | "right" | "top" | "bottom"
            ),
            other => config.placement() == other,
        };
        if !should_render || processor.toc_entries.is_empty() {
            return Ok(());
        }

        let w = self.writer_mut();
        w.queue(PrintStyledContent(
            config.title().unwrap_or("Table of Contents").bold(),
        ))?;
        writeln!(w)?;

        let part_signifier = match processor.document_attributes.get("part-signifier") {
            Some(AttributeValue::String(value)) => Some(value.as_ref()),
            Some(_) | None => None,
        };
        let numbering_config = NumberingConfig::new(&processor.document_attributes, part_signifier);
        let numbers = section_numbers(&processor.toc_entries, &numbering_config);
        let real_parts = has_real_parts(&processor.toc_entries);
        let first_level = processor
            .toc_entries
            .first()
            .map_or(1, |entry| effective_level(entry, real_parts));
        let parts_at_current_level = first_level > 0 && real_parts;
        let start_level = if parts_at_current_level {
            1
        } else {
            first_level
        };
        let render_config = TocRenderConfig {
            max_level: config.levels(),
            section_numbers: &numbers,
            has_real_parts: real_parts,
        };
        self.render_toc_entries(
            &processor.toc_entries,
            &render_config,
            start_level,
            0,
            parts_at_current_level,
            0,
        )?;
        writeln!(self.writer_mut())?;
        Ok(())
    }
}
