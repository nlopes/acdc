use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::Paragraph;

use crate::Error;

pub(crate) fn visit_paragraph<V: WritableVisitor<Error = Error>>(
    para: &Paragraph,
    visitor: &mut V,
) -> Result<(), Error> {
    visitor.visit_inline_nodes(&para.title)?;
    visitor.visit_inline_nodes(&para.content)?;
    let w = visitor.writer_mut();
    writeln!(w)?;
    Ok(())
}
