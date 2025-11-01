use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{DiscreteHeader, Section};

pub(crate) fn visit_section<V: WritableVisitor<Error = crate::Error>>(
    section: &Section,
    visitor: &mut V,
) -> Result<(), crate::Error> {
    let mut w = visitor.writer_mut();
    write!(w, "> ")?;
    let _ = w;
    for node in &section.title {
        visitor.visit_inline_node(node)?;
    }
    w = visitor.writer_mut();
    writeln!(w, " <")?;
    // Note: nested blocks are walked by the visitor itself
    Ok(())
}

pub(crate) fn visit_discrete_header<V: WritableVisitor<Error = crate::Error>>(
    header: &DiscreteHeader,
    visitor: &mut V,
) -> Result<(), crate::Error> {
    let mut w = visitor.writer_mut();
    write!(w, "> ")?;
    let _ = w;
    for node in &header.title {
        visitor.visit_inline_node(node)?;
    }
    w = visitor.writer_mut();
    writeln!(w, " <")?;
    Ok(())
}
