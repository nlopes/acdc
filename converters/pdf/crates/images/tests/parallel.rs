#![cfg(feature = "network")]

use std::{
    error::Error as StdError,
    fs, io,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use acdc_pdf_images::{Error, ResolveConfig, SourcePolicy, resolve};

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>"#;

#[test]
fn default_remote_fetches_overlap_two_at_a_time() -> Result<(), Box<dyn StdError>> {
    assert_remote_overlap(None, 2)
}

#[test]
fn delayed_responses_overlap_without_exceeding_worker_count() -> Result<(), Box<dyn StdError>> {
    assert_remote_overlap(Some(3), 3)
}

fn assert_remote_overlap(
    concurrency: Option<usize>,
    expected_peak: usize,
) -> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let arrivals = Arc::new((Mutex::new(0_usize), Condvar::new()));
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let peak_in_response = Arc::clone(&peak);
    let mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_chunked_body(move |writer| {
            peak_in_response.fetch_max(active.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
            let (count, ready) = &*arrivals;
            let mut count = count
                .lock()
                .map_err(|_| io::Error::other("arrival lock poisoned"))?;
            *count += 1;
            let target = count.div_ceil(expected_peak) * expected_peak;
            ready.notify_all();
            // Withhold each response until a full batch arrives. Sequential
            // fetching cannot complete this gate, regardless of machine speed.
            let (count, timeout) = ready
                .wait_timeout_while(count, Duration::from_secs(3), |count| *count < target)
                .map_err(|_| io::Error::other("arrival lock poisoned"))?;
            drop(count);
            if timeout.timed_out() {
                return Err(io::Error::other("requests did not overlap"));
            }
            active.fetch_sub(1, Ordering::SeqCst);
            writer.write_all(SVG.as_bytes())
        })
        .expect(6)
        .create();
    let root = tempfile::tempdir()?;
    let config = ResolveConfig::new(root.path(), root.path().join("spool"));
    let config = if let Some(count) = concurrency {
        config.with_remote_concurrency(count)
    } else {
        config
    };
    let mut urls = (0..6)
        .map(|index| format!("{}/{index}.svg", server.url()))
        .collect::<Vec<_>>();
    urls.insert(1, urls.first().ok_or("first URL missing")?.clone());
    let refs = urls.iter().map(String::as_str).collect::<Vec<_>>();

    let resolved = resolve(&refs, &config);

    mock.assert();
    assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
    assert_eq!(resolved.assets.images().count(), 6);
    assert_eq!(peak.load(Ordering::SeqCst), expected_peak);
    Ok(())
}

#[test]
fn parallel_fetches_keep_mixed_source_failures_in_input_order() -> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let missing = server
        .mock("GET", "/missing.svg")
        .with_status(404)
        .expect(1)
        .create();
    let valid = server
        .mock("GET", "/valid.svg")
        .with_body(SVG)
        .expect(1)
        .create();
    let invalid = server
        .mock("GET", "/invalid.svg")
        .with_body("not an image")
        .expect(1)
        .create();
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("local.svg"), SVG)?;
    let config =
        ResolveConfig::new(root.path(), root.path().join("spool")).with_remote_concurrency(3);
    let first = format!("{}/missing.svg", server.url());
    let second = format!("{}/valid.svg", server.url());
    let third = format!("{}/invalid.svg", server.url());
    let data = format!("data:image/svg+xml,{SVG}");

    let resolved = resolve(
        &[
            "missing-local.svg",
            &first,
            &second,
            &third,
            &data,
            "local.svg",
        ],
        &config,
    );

    missing.assert();
    valid.assert();
    invalid.assert();
    assert_eq!(resolved.assets.images().count(), 3);
    assert_eq!(
        resolved
            .failures
            .iter()
            .map(|failure| failure.url.as_str())
            .collect::<Vec<_>>(),
        ["missing-local.svg", first.as_str(), third.as_str()]
    );
    assert!(
        resolved
            .failures
            .iter()
            .any(|failure| matches!(failure.error, Error::HttpStatus(404)))
    );
    assert!(
        resolved
            .failures
            .iter()
            .any(|failure| matches!(failure.error, Error::UnknownFormat))
    );
    Ok(())
}

#[test]
fn byte_reservations_reduce_parallel_batch_and_do_not_fetch_later_sources()
-> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let first = server
        .mock("GET", "/first.svg")
        .with_body(SVG)
        .expect(1)
        .create();
    let second = server
        .mock("GET", "/second.svg")
        .with_body(SVG)
        .expect(1)
        .create();
    let skipped = server
        .mock("GET", "/skipped.svg")
        .with_body(SVG)
        .expect(0)
        .create();
    let urls = ["first", "second", "skipped"].map(|name| format!("{}/{name}.svg", server.url()));
    let refs = urls.iter().map(String::as_str).collect::<Vec<_>>();
    let root = tempfile::tempdir()?;
    let config = ResolveConfig::new(root.path(), root.path().join("spool"))
        .with_remote_concurrency(8)
        .with_max_bytes(SVG.len() as u64)
        .with_max_total_bytes(2 * SVG.len() as u64);

    let resolved = resolve(&refs, &config);

    first.assert();
    second.assert();
    skipped.assert();
    assert_eq!(resolved.assets.images().count(), 2);
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
fn oversized_parallel_response_does_not_cancel_valid_images() -> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let large = server
        .mock("GET", "/large.svg")
        .with_body(format!("{SVG}extra"))
        .expect(1)
        .create();
    let valid = server
        .mock("GET", "/valid.svg")
        .with_body(SVG)
        .expect(1)
        .create();
    let urls = ["large", "valid"].map(|name| format!("{}/{name}.svg", server.url()));
    let refs = urls.iter().map(String::as_str).collect::<Vec<_>>();
    let root = tempfile::tempdir()?;
    let config = ResolveConfig::new(root.path(), root.path().join("spool"))
        .with_remote_concurrency(2)
        .with_max_bytes(SVG.len() as u64);

    let resolved = resolve(&refs, &config);

    large.assert();
    valid.assert();
    assert_eq!(resolved.assets.images().count(), 1);
    assert!(
        resolved
            .failures
            .first()
            .is_some_and(|failure| matches!(failure.error, Error::TooLarge { .. }))
    );
    Ok(())
}

#[test]
fn source_quota_limits_parallel_requests_including_failures() -> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let missing = server
        .mock("GET", "/missing.svg")
        .with_status(404)
        .expect(1)
        .create();
    let valid = server
        .mock("GET", "/valid.svg")
        .with_body(SVG)
        .expect(1)
        .create();
    let skipped = server
        .mock("GET", "/skipped.svg")
        .with_body(SVG)
        .expect(0)
        .create();
    let urls = ["missing", "valid", "skipped"].map(|name| format!("{}/{name}.svg", server.url()));
    let refs = urls.iter().map(String::as_str).collect::<Vec<_>>();
    let root = tempfile::tempdir()?;
    let config = ResolveConfig::new(root.path(), root.path().join("spool"))
        .with_remote_concurrency(8)
        .with_max_sources(2);

    let resolved = resolve(&refs, &config);

    missing.assert();
    valid.assert();
    skipped.assert();
    assert_eq!(resolved.assets.images().count(), 1);
    assert_eq!(resolved.failures.len(), 2);
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
            .is_some_and(|failure| matches!(failure.error, Error::SourceLimit { limit: 2 }))
    );
    Ok(())
}

#[test]
fn parallel_setting_does_not_bypass_source_policy() -> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let skipped = server.mock("GET", mockito::Matcher::Any).expect(0).create();
    let root = tempfile::tempdir()?;
    let config = ResolveConfig::new(root.path(), root.path().join("spool"))
        .with_remote_concurrency(4)
        .with_source_policy(SourcePolicy::Confined {
            allow_network: false,
        });
    let first = format!("{}/first.svg", server.url());
    let second = format!("{}/second.svg", server.url());
    let data = format!("data:image/svg+xml,{SVG}");

    let resolved = resolve(&[&first, &second, &data], &config);

    skipped.assert();
    assert_eq!(resolved.assets.images().count(), 1);
    assert_eq!(resolved.failures.len(), 2);
    assert!(
        resolved
            .failures
            .iter()
            .all(|failure| matches!(failure.error, Error::AccessDenied(_)))
    );
    Ok(())
}

#[test]
#[ignore = "Manual latency measurement; does not assert timing in CI"]
fn remote_fetch_measurements() -> Result<(), Box<dyn StdError>> {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_chunked_body(|writer| {
            std::thread::sleep(Duration::from_millis(150));
            writer.write_all(SVG.as_bytes())
        })
        .expect(64)
        .create();
    let root = tempfile::tempdir()?;
    let urls = (0..4)
        .map(|index| format!("{}/{index}.svg", server.url()))
        .collect::<Vec<_>>();
    let refs = urls.iter().map(String::as_str).collect::<Vec<_>>();
    println!("pair,workers,seconds");
    for pair in 0..8 {
        let order = if pair % 2 == 0 { [1, 4] } else { [4, 1] };
        for workers in order {
            let config = ResolveConfig::new(root.path(), root.path().join("spool"))
                .with_remote_concurrency(workers);
            let start = Instant::now();
            let resolved = resolve(&refs, &config);
            let elapsed = start.elapsed();
            assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
            assert_eq!(resolved.assets.images().count(), 4);
            println!("{pair},{workers},{:.6}", elapsed.as_secs_f64());
        }
    }
    mock.assert();
    Ok(())
}
