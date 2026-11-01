use std::{io::Write, ops::Range};

use acdc_converters_core::{
    TraversalContext,
    section::book_chapter_signifier,
    toc::{Config as TocConfig, NumberingConfig, entry_level, has_parts, number_prefixes},
};
use acdc_parser::{
    SectionKind,
    ast::{TableOfContentsRef, TocEntryRef},
};

use crate::{Error, HtmlVariant, HtmlVisitor};

struct TocRenderConfig<'a> {
    max_level: u8,
    section_numbers: &'a [Option<String>],
    semantic: bool,
    /// Whether the document has normal level-0 parts.
    /// Controls where level-0 special sections are placed; see [`entry_level`].
    has_real_parts: bool,
}

/// Render TOC entries recursively.
///
/// When `parts_at_current_level` is true, level-0 entries (parts) are rendered
/// alongside level-1 entries in the same list. This matches asciidoctor behavior
/// when pre-part sections exist before the first level-0 section.
fn render_entries<'a, 'doc, W: Write>(
    traversal: &mut TraversalContext<'a>,
    entries: &(impl Iterator<Item = TocEntryRef<'doc>> + Clone),
    visitor: &mut HtmlVisitor<'a, '_, W>,
    config: &TocRenderConfig,
    current_level: u8,
    range: Range<usize>,
    parts_at_current_level: bool,
) -> Result<(), Error> {
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
            .clone()
            .enumerate()
            .skip(range.start)
            .take(range.len())
            .find(|(_, entry)| entry.level() == 0 && entry.kind() == SectionKind::Normal)
            .map(|(index, _)| index)
    } else {
        None
    };

    let current_level_entries: Vec<(usize, TocEntryRef<'doc>)> = entries
        .clone()
        .enumerate()
        .skip(range.start)
        .take(range.len())
        .filter(|(idx, entry)| {
            let eff_level = entry_level(*entry, config.has_real_parts);
            if eff_level == current_level {
                // When merging, only include level-1 entries before the first part
                if let Some(first_l0) = first_real_part_idx {
                    *idx < first_l0 || entry.level() != current_level
                } else {
                    true
                }
            } else {
                // Merge level-0 parts into the level-1 tier when required.
                parts_at_current_level && entry.level() == 0 && eff_level == 0
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
        write!(visitor.writer_mut(), "<a href=\"#{}\">", entry.id())?;

        // Include section number if available
        if let Some(Some(number)) = config.section_numbers.get(*entry_index) {
            write!(visitor.writer_mut(), "{number}")?;
        }

        // Enable TOC mode to render inline nodes without nested links
        let was_toc_mode = visitor.render_options.toc_mode;
        visitor.render_options.toc_mode = true;
        visitor.visit_title(traversal, entry.title())?;
        visitor.render_options.toc_mode = was_toc_mode;

        writeln!(visitor.writer_mut(), "</a>")?;
        // Find children: entries that come after this one but before the next
        // entry at the current tier
        let start_search = entry_index + 1;
        let end_search = if let Some(next_entry) = current_level_entries.get(i + 1) {
            next_entry.0 // Next entry at current level
        } else {
            range.end // End of this subtree
        };

        // A source level-0 special section is presented at the chapter tier, so
        // its valid children start at source level 2.
        let child_level = if entry.level() == 0 && entry.kind().is_special() {
            2
        } else {
            entry_level(*entry, config.has_real_parts) + 1
        };

        let children = start_search..end_search;
        let has_children = entries
            .clone()
            .skip(children.start)
            .take(children.len())
            .any(|entry| entry.level() == child_level);

        if has_children && child_level <= config.max_level {
            render_entries(
                traversal,
                entries,
                visitor,
                config,
                child_level,
                children,
                false, // no more merging in nested lists
            )?;
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

fn section_number_config<'a>(attributes: &'a TraversalContext<'_>) -> NumberingConfig<'a> {
    let part_signifier = attributes
        .get("part-signifier")
        .and_then(|value| value.text());
    let chapter_signifier = book_chapter_signifier(attributes, None);
    NumberingConfig::new(attributes, part_signifier, chapter_signifier)
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn render_toc(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        toc_macro: Option<TableOfContentsRef<'_>>,
        placement: &str,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        let config = TocConfig::resolve(
            toc_macro,
            &TraversalContext::new(processor.document_attributes()),
        );

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

            let entries = processor.toc_entries.iter().map(TocEntryRef::from);
            let section_numbers =
                number_prefixes(entries.clone(), &section_number_config(traversal));

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
            let has_real_parts = has_parts(entries.clone());
            let first_level = entries
                .clone()
                .next()
                .map_or(1, |entry| entry_level(entry, has_real_parts));
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
                traversal,
                &entries,
                self,
                &render_config,
                start_level,
                0..entries.len(),
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
