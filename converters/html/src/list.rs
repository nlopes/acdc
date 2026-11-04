use std::{io::Write, iter::Peekable};

use acdc_converters_core::{TraversalContext, list::OrderedListNumbering};
use acdc_parser::{
    ListItemCheckedStatus,
    ast::{
        BlockKind, BlockRef, CalloutListRef, DescriptionListRef, ListItemRef, OrderedListRef,
        UnorderedListRef,
    },
};

use crate::{
    Error, HtmlVariant, HtmlVisitor, build_class, image_helpers::attribute_text, write_element_id,
};

/// Check if any list item has a checkbox
fn has_checklist_items<'doc>(mut items: impl Iterator<Item = ListItemRef<'doc>>) -> bool {
    items.any(|item| item.checked().is_some())
}

/// The numbering style for a list at the given nesting `depth`.
/// Cycles through: arabic -> loweralpha -> lowerroman -> upperalpha -> upperroman -> arabic...
///
/// Note: This differs from asciidoctor, which stops cycling after depth 5 and uses
/// `arabic` for all depths > 5. We intentionally continue cycling to provide better
/// visual distinction in deeply nested lists (6+ levels). This is a design decision
/// that improves usability for deep hierarchies while maintaining consistency.
fn numbering_for_depth(depth: u8) -> OrderedListNumbering {
    match depth % 5 {
        2 => OrderedListNumbering::LowerAlpha, // a, b, c
        3 => OrderedListNumbering::LowerRoman, // i, ii, iii
        4 => OrderedListNumbering::UpperAlpha, // A, B, C
        0 => OrderedListNumbering::UpperRoman, // I, II, III (depth 5, 10, 15...)
        _ => OrderedListNumbering::Arabic,     // 1, 2, 3 (depth 1, 6, 11...)
    }
}

/// Map a numbering style to its CSS class and `<ol type>` value. The `(class, type)`
/// pairs match asciidoctor.
fn numbering_class_and_type(
    numbering: OrderedListNumbering,
) -> (&'static str, Option<&'static str>) {
    match numbering {
        OrderedListNumbering::Arabic => ("arabic", None),
        OrderedListNumbering::Decimal => ("decimal", None),
        OrderedListNumbering::LowerAlpha => ("loweralpha", Some("a")),
        OrderedListNumbering::UpperAlpha => ("upperalpha", Some("A")),
        OrderedListNumbering::LowerRoman => ("lowerroman", Some("i")),
        OrderedListNumbering::UpperRoman => ("upperroman", Some("I")),
        OrderedListNumbering::LowerGreek => ("lowergreek", None),
    }
}

/// Resolve an ordered list's CSS class and optional `<ol type>` value.
fn resolve_ordered_list_style(style: Option<&str>, depth: u8) -> (&str, Option<&'static str>) {
    if let Some(style) = style {
        return match OrderedListNumbering::from_explicit_style(style) {
            Some(numbering) => numbering_class_and_type(numbering),
            None => (style, None),
        };
    }

    numbering_class_and_type(numbering_for_depth(depth))
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn render_unordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: UnorderedListRef<'a>,
        section_style: Option<&str>,
    ) -> Result<(), Error> {
        let data = list;
        let is_checklist = has_checklist_items(data.items());
        let is_bibliography = section_style == Some("bibliography");

        let semantic = self.processor.variant() == HtmlVariant::Semantic;
        let has_title = data.title().is_some();

        // Semantic mode: use <section> for titled, <div> otherwise
        let wrapper_tag = if semantic && has_title {
            "section"
        } else {
            "div"
        };
        write!(self.writer, "<{wrapper_tag}")?;
        if let Some(id) = data.id() {
            crate::write_element_id(&mut self.writer, Some(id))?;
        }
        let mut wrapper_class = String::from("ulist");
        if is_checklist && !semantic {
            wrapper_class.push_str(" checklist");
        }
        if is_bibliography {
            wrapper_class.push_str(" bibliography");
        }
        if let Some(style) = data.style()
            && !wrapper_class.split_whitespace().any(|class| class == style)
        {
            wrapper_class.push(' ');
            wrapper_class.push_str(style);
        }
        wrapper_class = build_class(&wrapper_class, data.roles().iter());
        writeln!(self.writer, " class=\"{wrapper_class}\">")?;
        if semantic && has_title {
            self.render_title_content(
                traversal,
                data.title(),
                "<h6 class=\"block-title\">",
                "</h6>\n",
            )?;
        } else {
            self.render_title_content(
                traversal,
                data.title(),
                "<div class=\"title\">",
                "</div>\n",
            )?;
        }

        if is_checklist && semantic {
            writeln!(self.writer, "<ul class=\"task-list\">")?;
        } else if is_checklist {
            writeln!(self.writer, "<ul class=\"checklist\">")?;
        } else if is_bibliography {
            writeln!(self.writer, "<ul class=\"bibliography\">")?;
        } else if let Some(style) = data.style() {
            writeln!(self.writer, "<ul class=\"{style}\">")?;
        } else {
            writeln!(self.writer, "<ul>")?;
        }
        render_nested_list_items(
            traversal,
            &mut data.items().peekable(),
            self,
            1,
            false,
            1,
            !semantic,
        )?;
        writeln!(self.writer, "</ul>")?;
        writeln!(self.writer, "</{wrapper_tag}>")?;
        Ok(())
    }

    pub(crate) fn render_ordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: OrderedListRef<'a>,
    ) -> Result<(), Error> {
        let data = list;
        let raw_depth = data.marker().matches('.').count().max(1);
        if raw_depth > usize::from(u8::MAX) {
            self.diagnostics.warn_with_advice(
                format!("ordered list marker depth {raw_depth} exceeds 255, clamping"),
                "Reduce the ordered-list nesting depth to 255 levels or fewer.",
            );
        }
        let depth = u8::try_from(raw_depth).unwrap_or(u8::MAX);
        let (style, type_attr) = resolve_ordered_list_style(data.style(), depth);

        let semantic = self.processor.variant() == HtmlVariant::Semantic;
        let has_title = data.title().is_some();

        // Semantic mode: use <section> for titled, <div> otherwise
        let wrapper_tag = if semantic && has_title {
            "section"
        } else {
            "div"
        };
        write!(self.writer, "<{wrapper_tag}")?;
        if let Some(id) = data.id() {
            crate::write_element_id(&mut self.writer, Some(id))?;
        }
        let class = build_class(&format!("olist {style}"), data.roles().iter());
        writeln!(self.writer, " class=\"{class}\">")?;
        if semantic && has_title {
            self.render_title_content(
                traversal,
                data.title(),
                "<h6 class=\"block-title\">",
                "</h6>\n",
            )?;
        } else {
            self.render_title_content(
                traversal,
                data.title(),
                "<div class=\"title\">",
                "</div>\n",
            )?;
        }

        write!(self.writer, "<ol class=\"{style}\"")?;
        if let Some(value) = type_attr {
            write!(self.writer, " type=\"{value}\"")?;
        }
        if let Some(start) = attribute_text(data.attributes(), "start")
            .and_then(|start| start.parse::<usize>().ok())
            .filter(|start| *start > 0)
        {
            write!(self.writer, " start=\"{start}\"")?;
        }
        if data.options().contains("reversed") {
            write!(self.writer, " reversed")?;
        }
        writeln!(self.writer, ">")?;
        render_nested_list_items(
            traversal,
            &mut data.items().peekable(),
            self,
            1,
            true,
            1,
            !semantic,
        )?;
        writeln!(self.writer, "</ol>")?;
        writeln!(self.writer, "</{wrapper_tag}>")?;
        Ok(())
    }

    pub(crate) fn render_callout_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: CalloutListRef<'a>,
    ) -> Result<(), Error> {
        let data = list;
        if self.processor.variant() == HtmlVariant::Semantic {
            return visit_callout_list_semantic(traversal, list, self);
        }

        let class = build_class("colist arabic", data.roles().iter());
        write!(self.writer, "<div")?;
        write_element_id(&mut self.writer, data.id())?;
        writeln!(self.writer, " class=\"{class}\">")?;
        self.render_title_content(traversal, data.title(), "<div class=\"title\">", "</div>\n")?;

        if crate::callouts::font_icons(traversal) {
            writeln!(self.writer, "<table>")?;

            for item_data in data.items() {
                let num = item_data.callout().number();
                writeln!(self.writer, "<tr>")?;
                writeln!(
                    self.writer,
                    "<td><i class=\"conum\" data-value=\"{num}\"></i><b>{num}</b></td>"
                )?;
                write!(self.writer, "<td>")?;
                self.visit_content(traversal, item_data.principal())?;
                for block in item_data.blocks() {
                    traversal.visit_block(self, block)?;
                }
                writeln!(self.writer, "</td>")?;
                writeln!(self.writer, "</tr>")?;
            }

            writeln!(self.writer, "</table>")?;
        } else {
            writeln!(self.writer, "<ol>")?;

            for item_data in data.items() {
                write!(self.writer, "<li>")?;
                write!(self.writer, "<p>")?;
                self.visit_content(traversal, item_data.principal())?;
                write!(self.writer, "</p>")?;
                for block in item_data.blocks() {
                    traversal.visit_block(self, block)?;
                }
                writeln!(self.writer, "</li>")?;
            }

            writeln!(self.writer, "</ol>")?;
        }

        writeln!(self.writer, "</div>")?;
        Ok(())
    }
}

fn visit_callout_list_semantic<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    list: CalloutListRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let data = list;
    let has_title = data.title().is_some();
    if has_title {
        let writer = visitor.writer_mut();
        let class = build_class("colist arabic", data.roles().iter());
        write!(writer, "<section")?;
        write_element_id(writer, data.id())?;
        writeln!(writer, " class=\"{class}\">")?;
        let _ = writer;
        visitor.render_title_content(
            traversal,
            data.title(),
            "<h6 class=\"block-title\">",
            "</h6>\n",
        )?;
        writeln!(visitor.writer_mut(), "<ol class=\"callout-list arabic\">")?;
    } else {
        let writer = visitor.writer_mut();
        let class = build_class("callout-list arabic", data.roles().iter());
        write!(writer, "<ol")?;
        write_element_id(writer, data.id())?;
        writeln!(writer, " class=\"{class}\">")?;
    }

    for item_data in data.items() {
        let writer = visitor.writer_mut();
        write!(writer, "<li>")?;
        let _ = writer;
        visitor.visit_content(traversal, item_data.principal())?;
        for block in item_data.blocks() {
            traversal.visit_block(visitor, block)?;
        }
        let writer = visitor.writer_mut();
        writeln!(writer, "</li>")?;
    }

    let writer = visitor.writer_mut();
    writeln!(writer, "</ol>")?;
    if has_title {
        writeln!(writer, "</section>")?;
    }
    Ok(())
}

fn render_checked_status_list<W: Write + ?Sized>(
    is_ordered: bool,
    checked: Option<&ListItemCheckedStatus>,
    depth: u8,
    writer: &mut W,
    semantic: bool,
) -> Result<(), Error> {
    // Open nested list
    if is_ordered {
        let (style, type_attr) = numbering_class_and_type(numbering_for_depth(depth));
        if !semantic {
            write!(writer, "<div class=\"olist {style}")?;
            if checked.is_some() {
                writeln!(writer, " checklist\">")?;
            } else {
                writeln!(writer, "\">")?;
            }
        }

        write!(writer, "<ol class=\"{style}")?;
        if checked.is_some() {
            write!(writer, " checklist\"")?;
        } else {
            write!(writer, "\"")?;
        }
        if let Some(t) = type_attr {
            writeln!(writer, " type=\"{t}\">")?;
        } else {
            writeln!(writer, ">")?;
        }
    } else {
        if !semantic {
            // check if the item is a checkbox item
            write!(writer, "<div class=\"ulist")?;
            if checked.is_some() {
                writeln!(writer, " checklist\">")?;
            } else {
                writeln!(writer, "\">")?;
            }
        }
        write!(writer, "<ul")?;
        if checked.is_some() {
            if semantic {
                writeln!(writer, " class=\"task-list\">")?;
            } else {
                writeln!(writer, " class=\"checklist\">")?;
            }
        } else {
            writeln!(writer, ">")?;
        }
    }
    Ok(())
}

/// Render nested list items hierarchically
/// `depth` tracks the nesting level for ordered list style cycling (1 = top level)
#[tracing::instrument(skip(visitor, traversal, items))]
fn render_nested_list_items<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    items: &mut Peekable<impl Iterator<Item = ListItemRef<'a>>>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
    expected_level: u8,
    is_ordered: bool,
    depth: u8,
    wrap_li_in_p: bool,
) -> Result<(), Error> {
    let semantic = visitor.processor.variant() == HtmlVariant::Semantic;
    while let Some(item) = items.next_if(|item| item.level() >= expected_level) {
        render_list_item_content(traversal, item, visitor, wrap_li_in_p, semantic)?;
        if item.level() == expected_level
            && let Some(next) = items.peek().copied()
            && next.level() > expected_level
        {
            render_checked_status_list(
                is_ordered,
                next.checked().as_ref(),
                depth + 1,
                visitor.writer_mut(),
                !wrap_li_in_p,
            )?;
            render_nested_list_items(
                traversal,
                items,
                visitor,
                next.level(),
                is_ordered,
                depth + 1,
                wrap_li_in_p,
            )?;
            let writer = visitor.writer_mut();
            if is_ordered {
                writeln!(writer, "</ol>")?;
            } else {
                writeln!(writer, "</ul>")?;
            }
            if wrap_li_in_p {
                writeln!(writer, "</div>")?;
            }
        }
        writeln!(visitor.writer_mut(), "</li>")?;
    }
    Ok(())
}

/// Render the opening `<li>` tag, principal text, and attached blocks of a list item.
fn render_list_item_content<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    item: ListItemRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
    wrap_li_in_p: bool,
    semantic: bool,
) -> Result<(), Error> {
    let data = item;
    let writer = visitor.writer_mut();
    if semantic && data.checked().is_some() {
        writeln!(writer, "<li class=\"task-list-item\">")?;
    } else {
        writeln!(writer, "<li>")?;
    }
    if !data.principal().is_empty() || data.checked().is_some() {
        if wrap_li_in_p {
            write!(writer, "<p>")?;
        }
        render_checked_status(data.checked().as_ref(), writer, semantic)?;
        let _ = writer;
        visitor.visit_content(traversal, data.principal())?;
        let writer = visitor.writer_mut();
        if wrap_li_in_p {
            writeln!(writer, "</p>")?;
        }
        let _ = writer;
    } else {
        let _ = writer;
    }
    for block in data.blocks() {
        traversal.visit_block(visitor, block)?;
    }
    Ok(())
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn render_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        let data = list;
        let semantic = self.processor.variant() == HtmlVariant::Semantic;

        if semantic {
            return visit_description_list_semantic(traversal, list, self);
        }

        // Start the description list outer div
        write!(self.writer, "<div")?;
        if let Some(id) = data.id() {
            crate::write_element_id(&mut self.writer, Some(id))?;
        }

        // Description list
        let is_horizontal = data.style() == Some("horizontal");
        if is_horizontal {
            visit_horizontal_description_list(traversal, list, self)?;
        } else {
            visit_standard_description_list(traversal, list, self)?;
        }

        // Close the description list
        writeln!(self.writer, "</div>")?;
        Ok(())
    }
}

/// Renders a horizontal description list as an HTML table with `hdlist` class.
/// This matches asciidoctor's output for `[horizontal]` style description lists.
fn visit_horizontal_description_list<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    list: DescriptionListRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let data = list;
    let writer = visitor.writer_mut();

    let class = build_class("hdlist", data.roles().iter());
    writeln!(writer, " class=\"{class}\">")?;
    let _ = writer;
    visitor.render_title_content(traversal, data.title(), "<div class=\"title\">", "</div>\n")?;
    let mut writer = visitor.writer_mut();
    writeln!(writer, "<table>")?;
    let _ = writer;

    for item_data in data.items() {
        let mut writer = visitor.writer_mut();
        writeln!(writer, "<tr>")?;
        writeln!(writer, "<td class=\"hdlist1\">")?;
        let _ = writer;
        visitor.visit_content(traversal, item_data.term())?;
        writer = visitor.writer_mut();
        writeln!(writer, "</td>")?;
        writeln!(writer, "<td class=\"hdlist2\">")?;
        if !item_data.principal().is_empty() {
            write!(writer, "<p>")?;
            let _ = writer;
            visitor.visit_content(traversal, item_data.principal())?;
            writer = visitor.writer_mut();
            writeln!(writer, "</p>")?;
        }
        let _ = writer;
        for block in item_data.blocks() {
            traversal.visit_block(visitor, block)?;
        }
        writer = visitor.writer_mut();
        writeln!(writer, "</td>")?;
        writeln!(writer, "</tr>")?;
    }

    writer = visitor.writer_mut();
    writeln!(writer, "</table>")?;
    Ok(())
}

/// Renders a standard description list as an HTML `<dl>` with `dlist` class.
fn visit_standard_description_list<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    list: DescriptionListRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let data = list;
    let writer = visitor.writer_mut();

    // Check for ordered/unordered style (affects dt class)
    let is_marker_style = data
        .style()
        .is_some_and(|s| s == "ordered" || s == "unordered");

    // Build class including style if present
    let base_class = if let Some(style) = data.style() {
        format!("dlist {style}")
    } else {
        "dlist".to_string()
    };
    let class = build_class(&base_class, data.roles().iter());
    writeln!(writer, " class=\"{class}\">")?;
    let _ = writer;
    visitor.render_title_content(traversal, data.title(), "<div class=\"title\">", "</div>\n")?;
    let mut writer = visitor.writer_mut();
    writeln!(writer, "<dl>")?;
    let _ = writer;

    for item_data in data.items() {
        let mut writer = visitor.writer_mut();
        // Only add hdlist1 class when NOT ordered/unordered
        if is_marker_style {
            write!(writer, "<dt>")?;
        } else {
            write!(writer, "<dt class=\"hdlist1\">")?;
        }
        let _ = writer;
        visitor.visit_content(traversal, item_data.term())?;
        writer = visitor.writer_mut();
        writeln!(writer, "</dt>")?;
        writeln!(writer, "<dd>")?;
        if !item_data.principal().is_empty() {
            write!(writer, "<p>")?;
            let _ = writer;
            visitor.visit_content(traversal, item_data.principal())?;
            writer = visitor.writer_mut();
            writeln!(writer, "</p>")?;
        }
        let _ = writer;
        for block in item_data.blocks() {
            traversal.visit_block(visitor, block)?;
        }
        writer = visitor.writer_mut();
        writeln!(writer, "</dd>")?;
    }

    writer = visitor.writer_mut();
    writeln!(writer, "</dl>")?;
    Ok(())
}

/// Render a block inside a list item in semantic mode.
/// In semantic mode, list blocks (ulist, olist, dlist) inside list items
/// should not have their outer wrapper div/section — just the bare list element.
fn render_block_in_semantic_list_context<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    block: BlockRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    if let BlockKind::UnorderedList(list) = block.kind() {
        render_bare_ulist_semantic(traversal, list, visitor)
    } else if let BlockKind::OrderedList(list) = block.kind() {
        render_bare_olist_semantic(traversal, list, visitor)
    } else if let BlockKind::DescriptionList(list) = block.kind() {
        render_bare_dlist_semantic(traversal, list, visitor)
    } else {
        traversal.visit_block(visitor, block)
    }
}

/// Render an unordered list without the wrapper div/section (bare `<ul>` only).
fn render_bare_ulist_semantic<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    list: UnorderedListRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let data = list;
    let is_checklist = has_checklist_items(data.items());

    if is_checklist {
        writeln!(visitor.writer, "<ul class=\"task-list\">")?;
    } else if let Some(style) = data.style() {
        writeln!(visitor.writer, "<ul class=\"{style}\">")?;
    } else {
        writeln!(visitor.writer, "<ul>")?;
    }
    render_nested_list_items(
        traversal,
        &mut data.items().peekable(),
        visitor,
        1,
        false,
        1,
        false,
    )?;
    writeln!(visitor.writer, "</ul>")?;
    Ok(())
}

/// Render an ordered list without the wrapper div/section (bare `<ol>` only).
fn render_bare_olist_semantic<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    list: OrderedListRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let data = list;
    let raw_depth = data.marker().matches('.').count().max(1);
    let depth = u8::try_from(raw_depth).unwrap_or(u8::MAX);
    let (style, type_attr) = resolve_ordered_list_style(data.style(), depth);

    if let Some(t) = type_attr {
        writeln!(visitor.writer, "<ol class=\"{style}\" type=\"{t}\">")?;
    } else {
        writeln!(visitor.writer, "<ol class=\"{style}\">")?;
    }
    render_nested_list_items(
        traversal,
        &mut data.items().peekable(),
        visitor,
        1,
        true,
        1,
        false,
    )?;
    writeln!(visitor.writer, "</ol>")?;
    Ok(())
}

/// Render a description list without the wrapper div/section (bare `<dl>` only).
fn render_bare_dlist_semantic<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    list: DescriptionListRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let data = list;
    writeln!(visitor.writer, "<dl>")?;

    for item_data in data.items() {
        writeln!(visitor.writer, "<dt>")?;
        visitor.visit_content(traversal, item_data.term())?;
        writeln!(visitor.writer, "</dt>")?;

        if !item_data.principal().is_empty() || item_data.blocks().next().is_some() {
            writeln!(visitor.writer, "<dd>")?;
            if !item_data.principal().is_empty() {
                if item_data.blocks().next().is_none() {
                    visitor.visit_content(traversal, item_data.principal())?;
                } else {
                    write!(visitor.writer, "<p>")?;
                    visitor.visit_content(traversal, item_data.principal())?;
                    writeln!(visitor.writer, "</p>")?;
                }
            }
            for block in item_data.blocks() {
                render_block_in_semantic_list_context(traversal, block, visitor)?;
            }
            writeln!(visitor.writer, "</dd>")?;
        }
    }

    writeln!(visitor.writer, "</dl>")?;
    Ok(())
}

/// Render a description list in semantic HTML5 mode.
/// Key differences from standard mode:
/// - No `hdlist1` class on `<dt>` elements
/// - `<section>` wrapper for titled lists, `<div>` for untitled
/// - `<h6 class="block-title">` for titles
/// - qanda style gets `role="doc-qna"` and `<dl class="qanda">`
/// - horizontal style uses `<dl class="horizontal">` instead of `<table>`
/// - Simple `<dd>` text not wrapped in `<p>` unless there are also blocks
fn visit_description_list_semantic<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    list: DescriptionListRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let data = list;
    let has_title = data.title().is_some();
    let is_qanda = data.style() == Some("qanda");
    let is_horizontal = data.style() == Some("horizontal");

    // Build wrapper class
    let base_class = if let Some(style) = data.style() {
        format!("dlist {style}")
    } else {
        "dlist".to_string()
    };
    let class = build_class(&base_class, data.roles().iter());

    let wrapper_tag = if has_title { "section" } else { "div" };
    write!(visitor.writer, "<{wrapper_tag}")?;
    if let Some(id) = data.id() {
        crate::write_element_id(&mut visitor.writer, Some(id))?;
    }
    write!(visitor.writer, " class=\"{class}\"")?;
    if is_qanda {
        write!(visitor.writer, " role=\"doc-qna\"")?;
    }
    writeln!(visitor.writer, ">")?;

    if has_title {
        visitor.render_title_content(
            traversal,
            data.title(),
            "<h6 class=\"block-title\">",
            "</h6>\n",
        )?;
    }

    // Inner <dl> with optional class
    if is_qanda {
        writeln!(visitor.writer, "<dl class=\"qanda\">")?;
    } else if is_horizontal {
        writeln!(visitor.writer, "<dl class=\"horizontal\">")?;
    } else {
        writeln!(visitor.writer, "<dl>")?;
    }

    for item_data in data.items() {
        writeln!(visitor.writer, "<dt>")?;
        visitor.visit_content(traversal, item_data.term())?;
        writeln!(visitor.writer, "</dt>")?;

        // Only render <dd> if there's content
        if !item_data.principal().is_empty() || item_data.blocks().next().is_some() {
            writeln!(visitor.writer, "<dd>")?;
            if !item_data.principal().is_empty() {
                // Wrap in <p> only when there are also blocks
                if item_data.blocks().next().is_none() {
                    visitor.visit_content(traversal, item_data.principal())?;
                } else {
                    write!(visitor.writer, "<p>")?;
                    visitor.visit_content(traversal, item_data.principal())?;
                    writeln!(visitor.writer, "</p>")?;
                }
            }
            for block in item_data.blocks() {
                render_block_in_semantic_list_context(traversal, block, visitor)?;
            }
            writeln!(visitor.writer, "</dd>")?;
        }
    }

    writeln!(visitor.writer, "</dl>")?;
    writeln!(visitor.writer, "</{wrapper_tag}>")?;
    Ok(())
}

#[tracing::instrument(skip(w))]
fn render_checked_status<W: Write + ?Sized>(
    checked: Option<&ListItemCheckedStatus>,
    w: &mut W,
    semantic: bool,
) -> Result<(), Error> {
    if semantic {
        match checked {
            Some(ListItemCheckedStatus::Checked) => {
                write!(
                    w,
                    "<input class=\"task-list-item-checkbox\" type=\"checkbox\" disabled checked> "
                )?;
            }
            Some(ListItemCheckedStatus::Unchecked) => {
                write!(
                    w,
                    "<input class=\"task-list-item-checkbox\" type=\"checkbox\" disabled> "
                )?;
            }
            Some(_) | None => {}
        }
    } else {
        match checked {
            Some(ListItemCheckedStatus::Checked) => {
                write!(w, "&#10003; ")?;
            }
            Some(ListItemCheckedStatus::Unchecked) => {
                write!(w, "&#10063; ")?;
            }
            Some(_) | None => {}
        }
    }
    Ok(())
}
