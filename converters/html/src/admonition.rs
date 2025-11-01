use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::Admonition;

use crate::Error;

pub(crate) fn visit_admonition<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    admon: &Admonition,
) -> Result<(), Error> {
    let mut writer = visitor.writer_mut();
    writeln!(writer, "<div class=\"admonitionblock {}\">", admon.variant)?;
    writeln!(writer, "<table>")?;
    writeln!(writer, "<tr>")?;
    writeln!(writer, "<td class=\"icon\">")?;
    writeln!(writer, "<div class=\"title\">{}</div>", admon.variant)?;
    writeln!(writer, "</td>")?;
    writeln!(writer, "<td class=\"content\">")?;
    if !admon.title.is_empty() {
        write!(writer, "<div class=\"title\">")?;
        let _ = writer;
        visitor.visit_inline_nodes(&admon.title)?;
        writer = visitor.writer_mut();
        writeln!(writer, "</div>")?;
    }
    let _ = writer;
    for block in &admon.blocks {
        visitor.visit_block(block)?;
    }
    writer = visitor.writer_mut();
    writeln!(writer, "</td>")?;
    writeln!(writer, "</tr>")?;
    writeln!(writer, "</table>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}
