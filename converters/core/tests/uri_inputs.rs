//! URI helpers keep their results after temporary inputs are dropped.

use acdc_converters_core::{TraversalContext, icon::image_source, link::mailto_target};
use acdc_parser::{
    Location, Mailto, Options, Source,
    ast::{MailtoRef, SourceRef},
};

#[test]
fn mail_queries_follow_current_optional_values_and_outlive_them() {
    let (both, empty_body, absent_body, absent_subject) = {
        let target =
            String::from("team@example.test?SUBJECT=Old&%62ody=Old&cc=team%40example.test");
        let subject = String::from("One & two");
        let body = String::from("first\rsecond\nthird\r\nfourth");
        let mut mail = Mailto::new(Source::Name(&target), Location::default());
        mail.subject = Some(&subject);
        mail.body = Some(&body);
        let both = mailto_target(MailtoRef::from(&mail));
        mail.body = Some("");
        let empty_body = mailto_target(MailtoRef::from(&mail));
        mail.body = None;
        let absent_body = mailto_target(MailtoRef::from(&mail));
        mail.subject = None;
        mail.body = Some(&body);
        let absent_subject = mailto_target(MailtoRef::from(&mail));
        (both, empty_body, absent_body, absent_subject)
    };
    assert_eq!(
        both,
        "mailto:team@example.test?cc=team%40example.test&subject=One%20%26%20two&body=first%0D%0Asecond%0D%0Athird%0D%0Afourth"
    );
    assert_eq!(
        empty_body,
        "mailto:team@example.test?cc=team%40example.test&subject=One%20%26%20two&body="
    );
    assert_eq!(
        absent_body,
        "mailto:team@example.test?%62ody=Old&cc=team%40example.test&subject=One%20%26%20two"
    );
    assert_eq!(
        absent_subject,
        "mailto:team@example.test?SUBJECT=Old&%62ody=Old&cc=team%40example.test"
    );
}

#[test]
fn icon_image_sources_outlive_the_target_and_attribute_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let resolved = {
        let attributes = Options::builder()
            .with_attribute("iconsdir", "assets/icons/")
            .with_attribute("icontype", ".svg")
            .build()?
            .into_document_attributes();
        let target = String::from("arrow-left");
        let source = Source::Name(&target);
        image_source(
            &TraversalContext::new(&attributes),
            SourceRef::from(&source),
        )
    };
    assert_eq!(resolved, "assets/icons/arrow-left.svg");
    Ok(())
}
