use std::io::Write;

use acdc_converters_core::{
    section::book_chapter_signifier,
    toc::{Config as TocConfig, NumberingConfig, effective_level, has_real_parts, section_numbers},
    visitor::WritableVisitor,
};
use acdc_parser::{AttributeValue, SectionKind, TableOfContents, TocEntry};

use crate::{Error, HtmlVariant, HtmlVisitor};

struct TocRenderConfig<'a> {
    max_level: u8,
    section_numbers: &'a [Option<String>],
    semantic: bool,
    /// Whether the document has normal level-0 parts.
    /// Controls where level-0 special sections are placed; see [`effective_level`].
    has_real_parts: bool,
}

/// Render TOC entries recursively.
///
/// When `parts_at_current_level` is true, level-0 entries (parts) are rendered
/// alongside level-1 entries in the same list. This matches asciidoctor behavior
/// when pre-part sections exist before the first level-0 section.
fn render_entries<W: Write>(
    entries: &[TocEntry],
    visitor: &mut HtmlVisitor<'_, '_, W>,
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
    // Only normal level-0 sections are parts.
    let first_real_part_idx = if parts_at_current_level {
        entries
            .iter()
            .position(|entry| entry.level == 0 && entry.kind == SectionKind::Normal)
    } else {
        None
    };

    let current_level_entries: Vec<(usize, &TocEntry)> = entries
        .iter()
        .enumerate()
        .filter(|(idx, entry)| {
            let eff_level = effective_level(entry, config.has_real_parts);
            if eff_level == current_level {
                // When merging, only include level-1 entries before the first part
                if let Some(first_l0) = first_real_part_idx {
                    *idx < first_l0 || entry.level != current_level
                } else {
                    true
                }
            } else {
                // Merge level-0 parts into the level-1 tier when required.
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

        // A source level-0 special section is presented at the chapter tier, so
        // its valid children start at source level 2.
        let child_level = if entry.level == 0 && entry.kind.is_special() {
            2
        } else {
            effective_level(entry, config.has_real_parts) + 1
        };

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

fn section_number_config<'p>(processor: &'p crate::Processor<'_>) -> NumberingConfig<'p> {
    let attributes = processor.document_attributes();
    let part_signifier = match attributes.get("part-signifier") {
        Some(AttributeValue::String(value)) => Some(value.as_ref()),
        Some(_) | None => None,
    };
    let chapter_signifier = book_chapter_signifier(attributes, None);
    NumberingConfig::new(attributes, part_signifier, chapter_signifier)
}

impl<W: Write> HtmlVisitor<'_, '_, W> {
    pub(crate) fn render_toc(
        &mut self,
        toc_macro: Option<&TableOfContents>,
        placement: &str,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
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
            let embedded = self.render_options.embedded;

            // In embedded mode, sidebar positioning doesn't apply, so downgrade toc2 → toc
            let toc_class = if embedded && config.toc_class() == "toc2" {
                "toc"
            } else {
                config.toc_class()
            };

            // toc::[] macro adds class="title" to the toctitle div
            let is_macro = placement == "macro";

            let section_numbers = section_numbers(
                &processor.toc_entries,
                &section_number_config(&self.processor),
            );

            if semantic {
                writeln!(
                    self.writer_mut(),
                    "<nav id=\"toc\" class=\"{toc_class}\" role=\"doc-toc\">"
                )?;
                let title = config.title().unwrap_or("Table of Contents");
                writeln!(self.writer_mut(), "<h2 id=\"toc-title\">{title}</h2>")?;
            } else {
                writeln!(self.writer_mut(), "<div id=\"toc\" class=\"{toc_class}\">")?;
                let title_class = if is_macro { " class=\"title\"" } else { "" };
                if let Some(title) = config.title() {
                    writeln!(
                        self.writer_mut(),
                        "<div id=\"toctitle\"{title_class}>{title}</div>"
                    )?;
                } else {
                    writeln!(
                        self.writer_mut(),
                        "<div id=\"toctitle\"{title_class}>Table of Contents</div>"
                    )?;
                }
            }

            // Determine starting level: use the first entry's effective level.
            // Only normal level-0 sections count as parts. A level-0 special
            // section can sit at that tier without establishing a part.
            // When pre-part sections (level 1) appear before the first part (level 0),
            // the outer list starts at sectlevel1 and parts are merged into that tier.
            let has_real_parts = has_real_parts(&self.processor.toc_entries);
            let first_level = self
                .processor
                .toc_entries
                .first()
                .map_or(1, |entry| effective_level(entry, has_real_parts));
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
                has_real_parts,
            };
            render_entries(
                &processor.toc_entries,
                self,
                &render_config,
                start_level,
                0,
                parts_at_current_level,
            )?;
            if semantic {
                writeln!(self.writer_mut(), "</nav>")?;
            } else {
                writeln!(self.writer_mut(), "</div>")?;
            }
        }
        Ok(())
    }
}
