use acdc_parser::{Block, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn inline_roles_preserve_the_complete_first_positional_value() -> Result<(), Error> {
    for role in [
        "café",
        "école",
        "東京",
        "cafe\u{301}",
        "topic🚀",
        "123role",
        "_role",
        "two words",
        "café.résumé",
        "rôle#target",
        "%café",
        "!café",
        "café/role",
        "role_name",
        "café\"quote",
        "café&role",
        "café<role>",
    ] {
        for attribute in [role.to_owned(), "{role}".to_owned()] {
            for newline in ["\n", "\r\n"] {
                let source = format!(
                    ":role: {role}{newline}{newline}α [{attribute},ignored]**End** tail.{newline}"
                );
                let parsed = parse(&source, &Options::default())?;
                let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
                    return Err("missing paragraph".into());
                };
                let [
                    InlineNode::PlainText(prefix),
                    InlineNode::BoldText(bold),
                    InlineNode::PlainText(tail),
                ] = paragraph.content.as_slice()
                else {
                    return Err(format!("unexpected inlines: {source:?}: {paragraph:?}").into());
                };
                assert_eq!(prefix.content, "α ");
                assert_eq!(bold.role, Some(role), "{source:?}");
                assert_eq!(bold.id, None, "{source:?}");
                assert_eq!(tail.content, " tail.");
                let start = parsed.source().find("**End**").ok_or("missing source")?;
                assert_eq!(bold.location.absolute_start, start, "{source:?}");
                assert_eq!(bold.location.absolute_end, start + 6, "{source:?}");
                let [InlineNode::PlainText(text)] = bold.content.as_slice() else {
                    return Err("missing bold text".into());
                };
                assert_eq!(text.content, "End");
                assert_eq!(text.location.absolute_start, start + 2, "{source:?}");
                assert_eq!(text.location.absolute_end, start + 4, "{source:?}");
                assert_eq!(tail.location.absolute_start, start + 7, "{source:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn inline_unicode_ids_keep_their_full_target_and_roles() -> Result<(), Error> {
    for id in ["café", "école", "東京", "cafe\u{301}", "topic🚀", ":café"] {
        for attribute in [
            format!("#{id}"),
            format!("#{id}.résumé"),
            format!(".résumé#{id}"),
            "#{target}.résumé".to_owned(),
        ] {
            let source = format!(":target: {id}\n\nα [{attribute}]**End** tail.");
            let parsed = parse(&source, &Options::default())?;
            let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
                return Err("missing paragraph".into());
            };
            let [_, InlineNode::BoldText(bold), InlineNode::PlainText(tail)] =
                paragraph.content.as_slice()
            else {
                return Err(format!("unexpected inlines: {source:?}: {paragraph:?}").into());
            };
            assert_eq!(bold.id, Some(id), "{source:?}");
            assert_eq!(
                bold.role,
                attribute.contains("résumé").then_some("résumé"),
                "{source:?}"
            );
            let reference = parsed.document().references.get(id).ok_or("missing ID")?;
            assert_eq!(reference.location, bold.location, "{source:?}");
            let start = parsed.source().find("**End**").ok_or("missing source")?;
            assert_eq!(bold.location.absolute_start, start, "{source:?}");
            assert_eq!(bold.location.absolute_end, start + 6, "{source:?}");
            assert_eq!(tail.location.absolute_start, start + 7, "{source:?}");
        }
    }
    Ok(())
}
