use std::{error::Error as StdError, fs};

use acdc_pdf_images::{Error, ResolveConfig, SourcePolicy, resolve};

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>"#;
const SMALL_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg"/>"#;
const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABAQMAAAAl21bKAAAAA1BMVEXyVTNpJlJjAAAACklEQVQI12NgAAAAAgAB4iG8MwAAAABJRU5ErkJggg==";

fn config(root: &tempfile::TempDir) -> ResolveConfig {
    ResolveConfig::new(root.path(), root.path().join("spool"))
}

#[test]
fn source_limit_accepts_exact_count_and_counts_repeated_strings_once()
-> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("one.svg"), SVG)?;
    fs::write(root.path().join("two.svg"), SVG)?;
    fs::write(root.path().join("three.svg"), SVG)?;
    let mut config = config(&root);
    config.max_sources = 2;

    let resolved = resolve(
        &["one.svg", "one.svg", "two.svg", "three.svg", "three.svg"],
        &config,
    );

    assert_eq!(resolved.assets.images().count(), 2);
    assert!(resolved.assets.get("one.svg").is_some());
    assert!(resolved.assets.get("two.svg").is_some());
    assert_eq!(resolved.failures.len(), 1);
    assert!(resolved.failures.first().is_some_and(|failure| {
        failure.url == "three.svg" && matches!(failure.error, Error::SourceLimit { limit: 2 })
    }));
    Ok(())
}

#[test]
fn failed_sources_count_toward_source_limit() -> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("invalid.svg"), "not an image")?;
    let mut config = config(&root);
    config.max_sources = 2;
    let data = format!("data:image/svg+xml,{SVG}");

    let resolved = resolve(
        &["missing.svg", "missing.svg", "invalid.svg", &data],
        &config,
    );

    assert_eq!(resolved.assets.images().count(), 0);
    assert_eq!(resolved.failures.len(), 3);
    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::Io { .. }))
    );
    assert!(
        resolved
            .failures
            .iter()
            .any(|failure| matches!(failure.error, Error::UnknownFormat))
    );
    assert!(
        resolved
            .failures
            .last()
            .is_some_and(|failure| matches!(failure.error, Error::SourceLimit { limit: 2 }))
    );
    assert!(!config.spool_dir.exists());
    Ok(())
}

#[test]
fn zero_quotas_reject_sources_without_creating_snapshots() -> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    let mut config = config(&root);
    let data = format!("data:image/svg+xml,{SVG}");
    let urls = [
        "missing.svg",
        data.as_str(),
        "https://example.invalid/image.svg",
    ];

    config.max_sources = 0;
    let resolved = resolve(&urls, &config);
    assert_eq!(resolved.failures.len(), 3);
    assert!(
        resolved
            .failures
            .iter()
            .all(|failure| matches!(failure.error, Error::SourceLimit { limit: 0 }))
    );

    config.max_sources = 3;
    config.max_total_bytes = 0;
    let resolved = resolve(&urls, &config);
    assert_eq!(resolved.failures.len(), 3);
    assert!(
        resolved
            .failures
            .iter()
            .all(|failure| matches!(failure.error, Error::TotalBytesLimit { limit: 0 }))
    );
    assert!(!config.spool_dir.exists());
    Ok(())
}

#[test]
fn byte_limit_accepts_exact_total_across_files_and_padded_base64() -> Result<(), Box<dyn StdError>>
{
    use base64::Engine as _;

    let root = tempfile::tempdir()?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(PNG_B64)?;
    let file = root.path().join("one.png");
    fs::write(&file, &bytes)?;
    let file_url = url::Url::from_file_path(&file).map_err(|()| "could not make file URL")?;
    let data = format!("data:image/png;base64, {PNG_B64}\n");
    let mut config = config(&root);
    config.max_bytes = bytes.len() as u64;
    config.max_total_bytes = 2 * bytes.len() as u64;

    let resolved = resolve(&[file_url.as_str(), &data, &data, "missing.svg"], &config);

    let first = resolved
        .assets
        .get(file_url.as_str())
        .ok_or("file image missing")?;
    let second = resolved.assets.get(&data).ok_or("data image missing")?;
    assert_eq!(first.path, second.path);
    assert_eq!(fs::read_dir(&config.spool_dir)?.count(), 1);
    assert_eq!(resolved.failures.len(), 1);
    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::TotalBytesLimit { .. }))
    );
    Ok(())
}

#[test]
fn byte_overflow_stops_later_sources_even_if_they_would_fit() -> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("one.svg"), SVG)?;
    fs::write(
        root.path().join("two.svg"),
        SVG.replace("width=\"1\"", "width=\"2\""),
    )?;
    let small = format!("data:image/svg+xml,{SMALL_SVG}");
    let mut config = config(&root);
    config.max_total_bytes = 2 * SVG.len() as u64 - 1;
    assert!(SMALL_SVG.len() < SVG.len() - 1);

    let resolved = resolve(&["one.svg", "two.svg", &small], &config);

    assert_eq!(resolved.assets.images().count(), 1);
    assert!(resolved.assets.get("one.svg").is_some());
    assert_eq!(resolved.failures.len(), 2);
    assert!(
        resolved
            .failures
            .iter()
            .all(|failure| matches!(failure.error, Error::TotalBytesLimit { .. }))
    );
    assert_eq!(fs::read_dir(&config.spool_dir)?.count(), 1);
    Ok(())
}

#[test]
fn identical_content_at_different_sources_consumes_bytes_again() -> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    let first = format!("data:image/svg+xml,{SVG}");
    let second = format!("DATA:image/svg+xml,{SVG}");
    let mut config = config(&root);
    config.max_total_bytes = 2 * SVG.len() as u64 - 1;

    let resolved = resolve(&[&first, &second], &config);

    assert!(resolved.assets.get(&first).is_some());
    assert!(resolved.assets.get(&second).is_none());
    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::TotalBytesLimit { .. }))
    );
    assert_eq!(fs::read_dir(&config.spool_dir)?.count(), 1);
    Ok(())
}

#[test]
fn percent_encoded_data_accepts_exact_bytes_and_rejects_one_less() -> Result<(), Box<dyn StdError>>
{
    let root = tempfile::tempdir()?;
    let data = format!("data:image/svg+xml,{}", SVG.replace('<', "%3C"));
    let mut config = config(&root);
    config.max_total_bytes = SVG.len() as u64;

    let resolved = resolve(&[&data], &config);
    assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
    assert!(resolved.assets.get(&data).is_some());

    config.max_total_bytes -= 1;
    let resolved = resolve(&[&data], &config);
    assert_eq!(resolved.assets.images().count(), 0);
    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::TotalBytesLimit { .. }))
    );
    Ok(())
}

#[test]
fn invalid_and_oversized_images_do_not_spend_accepted_byte_allowance()
-> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    let invalid = "data:image/svg+xml,not an image";
    let large = format!("data:image/svg+xml,{SVG}{}", " ".repeat(100));
    let valid = format!("data:image/svg+xml,{SVG}");
    let mut config = config(&root);
    config.max_bytes = SVG.len() as u64;
    config.max_total_bytes = SVG.len() as u64;

    let resolved = resolve(&[invalid, &large, &valid], &config);

    assert_eq!(resolved.failures.len(), 2);
    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::UnknownFormat))
    );
    assert!(
        resolved
            .failures
            .last()
            .is_some_and(|failure| matches!(failure.error, Error::TooLarge { .. }))
    );
    assert!(resolved.assets.get(&valid).is_some());
    Ok(())
}

#[test]
fn quotas_restart_each_call_and_existing_snapshots_do_not_bypass_them()
-> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("one.svg"), SVG)?;
    fs::write(root.path().join("two.svg"), SVG)?;
    let mut config = config(&root);
    config.max_sources = 1;
    config.max_total_bytes = SVG.len() as u64;

    for _ in 0..2 {
        let resolved = resolve(&["one.svg"], &config);
        assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
    }
    config.max_sources = 2;
    let resolved = resolve(&["one.svg", "two.svg"], &config);
    assert!(resolved.assets.get("one.svg").is_some());
    assert!(resolved.assets.get("two.svg").is_none());
    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::TotalBytesLimit { .. }))
    );
    Ok(())
}

#[test]
fn source_policy_denials_count_as_attempts() -> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    let mut config = config(&root);
    config.source_policy = SourcePolicy::DenyAll;
    config.max_sources = 1;
    let data = format!("data:image/svg+xml,{SVG}");

    let resolved = resolve(&["one.svg", &data], &config);

    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::AccessDenied(_)))
    );
    assert!(
        resolved
            .failures
            .last()
            .is_some_and(|failure| matches!(failure.error, Error::SourceLimit { limit: 1 }))
    );
    assert!(!config.spool_dir.exists());
    Ok(())
}

#[cfg(feature = "network")]
#[test]
fn remote_failure_spends_source_allowance_and_later_urls_are_not_fetched()
-> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let missing = server
        .mock("GET", "/missing.svg")
        .with_status(404)
        .expect(1)
        .create();
    let skipped = server
        .mock("GET", "/skipped.svg")
        .with_status(200)
        .with_body(SVG)
        .expect(0)
        .create();
    let first = format!("{}/missing.svg", server.url());
    let second = format!("{}/skipped.svg", server.url());
    let root = tempfile::tempdir()?;
    let mut config = config(&root);
    config.max_sources = 1;

    let resolved = resolve(&[&first, &first, &second], &config);

    missing.assert();
    skipped.assert();
    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::HttpStatus(404)))
    );
    assert!(
        resolved
            .failures
            .last()
            .is_some_and(|failure| matches!(failure.error, Error::SourceLimit { limit: 1 }))
    );
    Ok(())
}

#[cfg(feature = "network")]
#[test]
fn remote_byte_overflow_stops_requests_after_partial_success() -> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let first_mock = server
        .mock("GET", "/first.svg")
        .with_status(200)
        .with_body(SVG)
        .expect(1)
        .create();
    let overflow = server
        .mock("GET", "/overflow.svg")
        .with_status(200)
        .with_body(SVG)
        .expect(1)
        .create();
    let skipped = server
        .mock("GET", "/skipped.svg")
        .with_status(200)
        .with_body(SMALL_SVG)
        .expect(0)
        .create();
    let first = format!("{}/first.svg", server.url());
    let second = format!("{}/overflow.svg", server.url());
    let third = format!("{}/skipped.svg", server.url());
    let root = tempfile::tempdir()?;
    let mut config = config(&root);
    config.max_total_bytes = 2 * SVG.len() as u64 - 1;

    let resolved = resolve(&[&first, &second, &third], &config);

    first_mock.assert();
    overflow.assert();
    skipped.assert();
    assert_eq!(resolved.assets.images().count(), 1);
    assert!(resolved.assets.get(&first).is_some());
    assert_eq!(resolved.failures.len(), 2);
    assert!(
        resolved
            .failures
            .iter()
            .all(|failure| matches!(failure.error, Error::TotalBytesLimit { .. }))
    );
    assert_eq!(fs::read_dir(&config.spool_dir)?.count(), 1);
    Ok(())
}

#[cfg(feature = "network")]
#[test]
fn remote_image_at_exact_byte_limit_succeeds_without_fetching_next_url()
-> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let first_mock = server
        .mock("GET", "/first.svg")
        .with_status(200)
        .with_body(SVG)
        .expect(1)
        .create();
    let skipped = server
        .mock("GET", "/skipped.svg")
        .with_status(200)
        .with_body(SVG)
        .expect(0)
        .create();
    let first = format!("{}/first.svg", server.url());
    let second = format!("{}/skipped.svg", server.url());
    let root = tempfile::tempdir()?;
    let mut config = config(&root);
    config.max_total_bytes = SVG.len() as u64;

    let resolved = resolve(&[&first, &second], &config);

    first_mock.assert();
    skipped.assert();
    assert!(resolved.assets.get(&first).is_some());
    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::TotalBytesLimit { .. }))
    );
    Ok(())
}
