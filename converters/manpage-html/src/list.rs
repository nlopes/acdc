use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor, WritableVisitorExt};
use acdc_parser::{
    CalloutList, DescriptionList, ListItemCheckedStatus, OrderedList, UnorderedList,
};

use crate::{Error, ManpageHtmlVisitor};

pub(crate) fn visit_unordered_list<W: Write>(
    list: &UnorderedList,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    visitor.render_title_with_wrapper(&list.title, "<p class=\"Pp\"><b>", "</b></p>")?;

    write!(visitor.writer_mut(), "<ul>")?;
    visitor.list_depth += 1;

    for item in &list.items {
        write!(visitor.writer_mut(), "<li>")?;

        if let Some(checked) = &item.checked {
            match checked {
                ListItemCheckedStatus::Checked => {
                    write!(
                        visitor.writer_mut(),
                        "<input type=\"checkbox\" checked disabled> "
                    )?;
                }
                ListItemCheckedStatus::Unchecked => {
                    write!(visitor.writer_mut(), "<input type=\"checkbox\" disabled> ")?;
                }
                _ => {}
            }
        }

        if !item.principal.is_empty() {
            visitor.visit_inline_nodes(&item.principal)?;
        }

        for block in &item.blocks {
            visitor.visit_block(block)?;
        }

        write!(visitor.writer_mut(), "</li>")?;
    }

    visitor.list_depth -= 1;
    write!(visitor.writer_mut(), "</ul>")?;

    Ok(())
}

pub(crate) fn visit_ordered_list<W: Write>(
    list: &OrderedList,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    visitor.render_title_with_wrapper(&list.title, "<p class=\"Pp\"><b>", "</b></p>")?;

    write!(visitor.writer_mut(), "<ol>")?;
    visitor.list_depth += 1;

    for item in &list.items {
        write!(visitor.writer_mut(), "<li>")?;

        if !item.principal.is_empty() {
            visitor.visit_inline_nodes(&item.principal)?;
        }

        for block in &item.blocks {
            visitor.visit_block(block)?;
        }

        write!(visitor.writer_mut(), "</li>")?;
    }

    visitor.list_depth -= 1;
    write!(visitor.writer_mut(), "</ol>")?;

    Ok(())
}

pub(crate) fn visit_description_list<W: Write>(
    list: &DescriptionList,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    visitor.render_title_with_wrapper(&list.title, "<p class=\"Pp\"><b>", "</b></p>")?;

    write!(visitor.writer_mut(), "<dl class=\"Bl-tag\">")?;
    visitor.list_depth += 1;

    for item in &list.items {
        write!(visitor.writer_mut(), "<dt><b>")?;
        visitor.visit_inline_nodes(&item.term)?;
        write!(visitor.writer_mut(), "</b></dt>")?;

        write!(visitor.writer_mut(), "<dd>")?;
        if !item.principal_text.is_empty() {
            visitor.visit_inline_nodes(&item.principal_text)?;
        }
        for block in &item.description {
            visitor.visit_block(block)?;
        }
        write!(visitor.writer_mut(), "</dd>")?;
    }

    visitor.list_depth -= 1;
    write!(visitor.writer_mut(), "</dl>")?;

    Ok(())
}

pub(crate) fn visit_callout_list<W: Write>(
    list: &CalloutList,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    visitor.render_title_with_wrapper(&list.title, "<p class=\"Pp\"><b>", "</b></p>")?;

    write!(visitor.writer_mut(), "<ol class=\"callout-list\">")?;
    visitor.list_depth += 1;

    for item in &list.items {
        write!(visitor.writer_mut(), "<li>")?;

        if !item.principal.is_empty() {
            visitor.visit_inline_nodes(&item.principal)?;
        }

        for block in &item.blocks {
            visitor.visit_block(block)?;
        }

        write!(visitor.writer_mut(), "</li>")?;
    }

    visitor.list_depth -= 1;
    write!(visitor.writer_mut(), "</ol>")?;

    Ok(())
}
