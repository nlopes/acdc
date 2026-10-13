use acdc_pdf_theme::{
    CaptionFontStyle, Error, PageBreakBefore, PageOrientation, Theme, asciidoctor,
};

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn asciidoctor_theme_code_sizes_do_not_warn_for_an_explicit_valid_minimum() -> Result<(), Error> {
    for source in [
        "base_font_size: 12\ncode_font_size: 5\ncode_font_size_min: 4\n",
        "base_font_size: 12\ncode_font_size_min: 4\ncode_font_size: 5\n",
        "base:\n  font_size: 12\ncode:\n  font_size_min: 4\n  font_size: 5\n",
    ] {
        let converted = asciidoctor::from_yaml_str(source)?;
        close(converted.theme.typography.code_size_em, 5.0 / 12.0);
        close(converted.theme.typography.code_min_size_em, 4.0 / 12.0);
        assert!(
            converted.warnings.is_empty(),
            "{source}: {:?}",
            converted.warnings
        );
    }
    Ok(())
}

#[test]
fn asciidoctor_theme_code_sizes_adjust_only_the_default_minimum()
-> Result<(), Box<dyn std::error::Error>> {
    let converted = asciidoctor::from_yaml_str("base_font_size: 12\ncode_font_size: 5\n")?;
    close(converted.theme.typography.code_size_em, 5.0 / 12.0);
    close(converted.theme.typography.code_min_size_em, 5.0 / 12.0);
    let [warning] = converted.warnings.as_slice() else {
        return Err("expected one code minimum adjustment warning".into());
    };
    assert_eq!(warning.key, "code_font_size");
    assert!(warning.message.contains("minimum autofit size was reduced"));
    let error = asciidoctor::from_yaml_str(
        "base_font_size: 12\ncode_font_size: 5\ncode_font_size_min: 6\n",
    )
    .err()
    .ok_or("expected an invalid code minimum error")?;
    assert!(error.to_string().contains("code_font_size_min"));
    Ok(())
}

#[test]
fn asciidoctor_theme_heading_sizes_follow_section_roles() -> Result<(), Error> {
    let converted = asciidoctor::from_yaml_str(
        "heading_h1_font_size: 36\nheading_h2_font_size: 20\nheading_h3_font_size: 10\nheading_h6_font_size: 8\n",
    )?;
    close(converted.theme.typography.heading_pt[0], 20.0);
    close(converted.theme.typography.heading_pt[1], 10.0);
    close(converted.theme.typography.heading_pt[4], 8.0);
    assert!(converted.warnings.iter().any(|warning| {
        warning.key == "heading_h1_font_size"
            && warning.message.contains("document-title")
            && warning.message.contains("book-part")
    }));
    Ok(())
}

#[test]
fn asciidoctor_theme_quoted_rgb_variables_keep_leading_zeros() -> Result<(), Error> {
    for (value, expected) in [
        ("000000", "#000000"),
        ("000123", "#000123"),
        ("123456", "#123456"),
        ("001", "#000011"),
    ] {
        let converted = asciidoctor::from_yaml_str(&format!(
            "brand: '{value}'\ncopy: $brand\nbase_font_color: $copy\nlink_font_color: $brand\n",
        ))?;
        assert_eq!(converted.theme.palette.body_text, expected);
        assert_eq!(converted.theme.palette.link, expected);
    }
    let converted = asciidoctor::from_yaml_str(
        "size: '0012'\nbase_font_size: $size\nheading_h2_font_size: $size * 2\n",
    )?;
    close(converted.theme.typography.body_size_pt, 12.0);
    close(converted.theme.typography.heading_pt[0], 24.0);
    Ok(())
}

#[test]
fn asciidoctor_theme_custom_settings_keep_inherited_defaults() -> Result<(), Error> {
    let inherited = asciidoctor::from_yaml_str("extends: default\n")?.theme;
    let mut expected = inherited.clone();
    "#008800".clone_into(&mut expected.palette.link);
    let custom =
        asciidoctor::from_yaml_str("extends: default\nlink:\n  font_color: '008800'\n")?.theme;
    assert_eq!(custom, expected);
    assert_eq!(custom.typography.body_size_pt, 10.5);
    Ok(())
}

#[test]
fn asciidoctor_theme_export_preserves_tokens_and_page_defaults() -> Result<(), Error> {
    for source in [
        include_str!("fixtures/asciidoctor-brand.yml"),
        "extends: default\nlink_font_color: '008800'\n",
        "page_size: Letter\npage_margin: 36\n",
    ] {
        let theme = asciidoctor::from_yaml_str(source)?.theme;
        let yaml = theme.to_yaml_string()?;
        assert_eq!(Theme::from_yaml_str(&yaml)?, theme);
        assert_eq!(theme.to_yaml_string()?, yaml);
        assert!(!yaml.contains("extends:"));
        assert!(!yaml.contains("$base_font_size"));
    }
    Ok(())
}

#[test]
fn asciidoctor_theme_converts_brand_settings_and_page_geometry() -> Result<(), Error> {
    let converted = asciidoctor::from_yaml_str(include_str!("fixtures/asciidoctor-brand.yml"))?;
    close(converted.theme.typography.body_size_pt, 12.0);
    close(converted.theme.typography.code_size_em, 0.75);
    close(
        converted
            .theme
            .typography
            .heading_pt
            .first()
            .copied()
            .unwrap_or_default(),
        18.0,
    );
    close(converted.theme.caption.font_size_em, 10.0 / 12.0);
    assert_eq!(converted.theme.caption.font_weight, 700);
    assert_eq!(converted.theme.caption.font_style, CaptionFontStyle::Italic);
    assert_eq!(
        converted.theme.heading.chapter.break_before,
        PageBreakBefore::Auto
    );
    assert_eq!(converted.theme.palette.body_text, "#000000");
    assert_eq!(converted.theme.palette.link, "#008800");
    assert_eq!(converted.theme.typography.body_font.brand, None);
    assert_eq!(converted.theme.index.columns, 3);
    close(
        converted.theme.index.column_gap_pt.unwrap_or_default(),
        36.0,
    );
    assert_eq!(converted.theme.page.size_pt, Some([432.0, 648.0]));
    assert_eq!(
        converted.theme.page.layout,
        Some(PageOrientation::Landscape)
    );
    let margins = converted
        .theme
        .page
        .margin_pt
        .ok_or_else(|| Error::Validation {
            field: "page.margin".into(),
            message: "missing margins".into(),
        })?;
    for (actual, expected) in margins
        .into_iter()
        .zip([36.0, 72.0 / 2.54, 54.0, 144.0 / 2.54])
    {
        close(actual, expected);
    }
    Ok(())
}

#[test]
fn asciidoctor_theme_key_spellings_and_nesting_are_equivalent() -> Result<(), Error> {
    let nested = asciidoctor::from_yaml_str("base:\n  font-size: 14\nlink:\n  font-color: ABC\n")?;
    let flat = asciidoctor::from_yaml_str("base_font_size: 14\nlink-font-color: ABC\n")?;
    assert_eq!(nested.theme, flat.theme);
    assert_eq!(nested.theme.palette.link, "#aabbcc");
    Ok(())
}

#[test]
fn asciidoctor_theme_references_keep_the_value_at_the_assignment() -> Result<(), Error> {
    let converted = asciidoctor::from_yaml_str(
        "extends: default\nbase:\n  font_size: 21\nheading:\n  h2_font_size: $base_font_size + 1\n",
    )?;
    close(converted.theme.typography.body_size_pt, 21.0);
    close(
        converted
            .theme
            .typography
            .heading_pt
            .first()
            .copied()
            .unwrap_or_default(),
        22.0,
    );
    close(
        converted
            .theme
            .typography
            .heading_pt
            .get(1)
            .copied()
            .unwrap_or_default(),
        18.0,
    );
    close(converted.theme.typography.code_size_em, 11.0 / 21.0);
    Ok(())
}

#[test]
fn asciidoctor_theme_resolves_arithmetic_units_and_rounding() -> Result<(), Error> {
    let converted = asciidoctor::from_yaml_str(
        "base_font_size: 12\nheading_h1_font_size: floor($base_font_size * 2.6)\nheading_h2_font_size: ceil(18.2)\nheading_h3_font_size: round(13.5)\ncaption_margin_inside: (1in - 0.5in) / 3\n",
    )?;
    let first: Vec<_> = converted
        .theme
        .typography
        .heading_pt
        .into_iter()
        .take(2)
        .collect();
    assert_eq!(first, [19.0, 14.0]);
    close(converted.theme.caption.margin_inside_pt, 12.0);
    Ok(())
}

#[test]
fn asciidoctor_theme_reports_all_unmapped_leaf_paths() -> Result<(), Error> {
    let converted = asciidoctor::from_yaml_str(
        "font:\n  catalog:\n    Brand:\n      normal: brand.ttf\nrole:\n  lead:\n    font_size: 16\ntitle_page:\n  authors:\n    content: '{author}'\nempty: {}\n",
    )?;
    let paths: Vec<_> = converted
        .warnings
        .iter()
        .map(|warning| warning.key.as_str())
        .collect();
    for expected in [
        "font.catalog.Brand.normal",
        "role.lead.font_size",
        "title_page.authors.content",
        "empty",
    ] {
        assert!(paths.contains(&expected), "{paths:?}");
    }
    Ok(())
}

#[test]
fn asciidoctor_theme_external_extends_is_reported_without_loading() -> Result<(), Error> {
    let converted =
        asciidoctor::from_yaml_str("extends: ./missing-theme.yml\nbase_font_size: 15\n")?;
    close(converted.theme.typography.body_size_pt, 15.0);
    assert!(
        converted
            .warnings
            .iter()
            .any(|warning| warning.key == "extends" && warning.message.contains("not loaded"))
    );
    Ok(())
}

#[test]
fn asciidoctor_theme_forward_reference_does_not_keep_an_inherited_override() -> Result<(), Error> {
    let converted =
        asciidoctor::from_yaml_str("extends: default\nlink_font_color: $later\nlater: FF0000\n")?;
    assert_eq!(converted.theme.palette.link, "#2563eb");
    assert!(
        converted
            .warnings
            .iter()
            .any(|warning| warning.key == "link_font_color"
                && warning.message.contains("define it before"))
    );
    Ok(())
}

#[test]
fn asciidoctor_theme_preserves_unquoted_numeric_colour_width() -> Result<(), Error> {
    for (input, expected) in [
        ("000000", "#000000"),
        ("012345", "#012345"),
        ("123", "#112233"),
        ("000", "#000000"),
    ] {
        let converted = asciidoctor::from_yaml_str(&format!("link_font_color: {input}\n"))?;
        assert_eq!(converted.theme.palette.link, expected, "{input}");
    }
    Ok(())
}

#[test]
fn asciidoctor_theme_invalid_supported_settings_name_the_source_key()
-> Result<(), Box<dyn std::error::Error>> {
    for (input, key) in [
        ("base:\n  font_size: -1", "base.font_size"),
        ("link_font_color: xyz", "link_font_color"),
        ("page_margin: [-1, 2]", "page_margin"),
        ("index_columns: 1.5", "index_columns"),
        ("base_font_family: ' '", "base_font_family"),
    ] {
        let error = asciidoctor::from_yaml_str(input)
            .err()
            .ok_or("expected an invalid theme error")?;
        assert!(error.to_string().contains(key), "{error}");
    }
    Ok(())
}

#[test]
fn asciidoctor_theme_rejects_duplicate_yaml_keys_and_multiple_documents() {
    for input in [
        "base_font_size: 12\nbase_font_size: 14",
        "base_font_size: 12\n---\nbase_font_size: 14",
        "- theme",
        "null",
    ] {
        assert!(asciidoctor::from_yaml_str(input).is_err(), "{input}");
    }
}

#[test]
fn asciidoctor_theme_translation_is_deterministic_for_upstream_themes() -> Result<(), Error> {
    for input in [
        include_str!("../assets/asciidoctor/base-theme.yml"),
        include_str!("../assets/asciidoctor/default-theme.yml"),
        include_str!("fixtures/asciidoctor-brand.yml"),
    ] {
        assert_eq!(
            asciidoctor::from_yaml_str(input)?,
            asciidoctor::from_yaml_str(input)?
        );
    }
    Ok(())
}

#[test]
fn asciidoctor_theme_colour_aliases_and_unicode_keep_source_width() -> Result<(), Error> {
    let converted =
        asciidoctor::from_yaml_str("label: café\nshade: &brand 001234\nlink_font_color: *brand\n")?;
    assert_eq!(converted.theme.palette.link, "#001234");
    Ok(())
}

#[test]
fn asciidoctor_theme_unsupported_colour_and_padding_values_warn() -> Result<(), Error> {
    let converted = asciidoctor::from_yaml_str(
        "page_background_color: transparent\nlink_font_color: [0, 100, 0, 0]\ncode_padding: [1, 2]\ncaption_font_color: null\n",
    )?;
    for key in ["page_background_color", "link_font_color", "code_padding"] {
        assert!(
            converted.warnings.iter().any(|warning| warning.key == key),
            "{}",
            key
        );
    }
    assert_eq!(converted.theme.caption.font_color, None);
    Ok(())
}

#[test]
fn asciidoctor_theme_reference_expansion_is_bounded() -> Result<(), Error> {
    let text = "x".repeat(4097);
    let converted =
        asciidoctor::from_yaml_str(&format!("large: {text}\nbase_font_family: $large\n"))?;
    assert!(
        converted.warnings.iter().any(
            |warning| warning.key == "base_font_family" && warning.message.contains("too long")
        )
    );
    Ok(())
}

#[test]
fn asciidoctor_theme_numeric_measurements_keep_yaml_numeric_meaning() -> Result<(), Error> {
    let converted = asciidoctor::from_yaml_str(
        "base_font_size: 0xC\ncode_font_size: 1.0e1\nlink_font_color: 000123\n",
    )?;
    close(converted.theme.typography.body_size_pt, 12.0);
    close(converted.theme.typography.code_size_em, 10.0 / 12.0);
    assert_eq!(converted.theme.palette.link, "#000123");
    Ok(())
}
