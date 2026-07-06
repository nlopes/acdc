use acdc_pdf_theme::embedded_fonts;
use ttf_parser::{Face, name_id};

const EXPECTED_FACES: &[(&str, u16, bool)] = &[
    ("IBM Plex Sans", 400, false),
    ("IBM Plex Sans", 400, true),
    ("IBM Plex Sans", 500, false),
    ("IBM Plex Sans", 600, false),
    ("IBM Plex Sans", 700, false),
    ("IBM Plex Serif", 400, false),
    ("IBM Plex Serif", 500, false),
    ("IBM Plex Serif", 700, false),
    ("IBM Plex Serif", 400, true),
    ("IBM Plex Mono", 400, false),
    ("IBM Plex Mono", 700, false),
    ("Noto Color Emoji", 400, false),
];

#[test]
fn bundled_fonts_have_the_expected_metadata() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(embedded_fonts().len(), EXPECTED_FACES.len());

    for (bytes, &(family, weight, italic)) in embedded_fonts().iter().zip(EXPECTED_FACES) {
        let face = Face::parse(bytes, 0)?;
        assert_eq!(font_family(&face).as_deref(), Some(family));
        assert_eq!(face.weight().to_number(), weight);
        assert_eq!(face.is_italic(), italic);
    }
    Ok(())
}

fn font_family(face: &Face<'_>) -> Option<String> {
    [name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY]
        .into_iter()
        .find_map(|name_id| {
            face.names()
                .into_iter()
                .filter(|name| name.name_id == name_id)
                .find_map(|name| name.to_string())
        })
}
