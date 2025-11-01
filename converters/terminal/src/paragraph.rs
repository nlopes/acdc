use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::Paragraph;

use crate::Error;

pub(crate) fn visit_paragraph<V: WritableVisitor<Error = Error>>(
    para: &Paragraph,
    visitor: &mut V,
) -> Result<(), Error> {
    visitor.visit_inline_nodes(&para.title)?;

    let last_index = para.content.len() - 1;
    for (i, node) in para.content.iter().enumerate() {
        visitor.visit_inline_node(node)?;
        if i != last_index {
            let w = visitor.writer_mut();
            write!(w, " ")?;
        }
    }
    let w = visitor.writer_mut();
    writeln!(w)?;
    writeln!(w)?;
    Ok(())
}
