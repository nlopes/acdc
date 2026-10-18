use std::{fs, path::Path};

use acdc_converters_core::{Diagnostics, Options, Warning, WarningSource};
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::{Options as ParserOptions, SafeMode, parse};
use base64::{Engine as _, engine::general_purpose::STANDARD};

type Error = Box<dyn std::error::Error>;

const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"/></svg>";

fn convert(
    source: &str,
    directory: &Path,
    safe_mode: SafeMode,
    allow_uri_read: bool,
) -> Result<(String, Vec<Warning>), Error> {
    let parser_options = ParserOptions::builder()
        .with_attribute("allow-uri-read", allow_uri_read)
        .build()?;
    let parsed = parse(source, &parser_options)?;
    let processor = Processor::new_with_variant(
        Options::builder().safe_mode(safe_mode).build(),
        ParserOptions::builder()
            .with_attributes(parsed.document().attributes.clone().into_inputs()),
        HtmlVariant::Standard,
    )?;
    let options = RenderOptions {
        embedded: true,
        source_dir: Some(directory.to_owned()),
        ..RenderOptions::default()
    };
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("html");
    processor.convert_to_writer(
        parsed.document(),
        &mut output,
        &options,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    Ok((String::from_utf8(output)?, warnings))
}

#[test]
fn embedded_image_bytes_and_secure_mode() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    fs::create_dir(directory.path().join("media"))?;
    fs::write(directory.path().join("media/my image.svg"), SVG)?;
    let document = ":data-uri:\n:imagesdir: media\n\nimage::my image.svg[Alt]\n\nimage::my image.svg[Alt,opts=inline]\n\nimage::my image.svg[Alt,opts=interactive]\n";
    let (html, warnings) = convert(document, directory.path(), SafeMode::Safe, false)?;
    assert!(warnings.is_empty(), "{warnings:?}");
    let encoded = html
        .split("base64,")
        .nth(1)
        .and_then(|tail| tail.split('"').next())
        .ok_or("missing data URI")?;
    assert_eq!(STANDARD.decode(encoded)?, SVG.as_bytes());
    assert!(html.contains("<svg "));
    assert!(html.contains("<object "));

    let (html, warnings) = convert(document, directory.path(), SafeMode::Secure, false)?;
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!html.contains("data:"));
    assert!(!html.contains("<svg") && !html.contains("<object"));
    assert_eq!(html.matches("src=\"media/my%20image.svg\"").count(), 3);
    Ok(())
}

#[test]
fn image_embedding_denies_parent_paths_and_reports_missing_or_oversized_files() -> Result<(), Error>
{
    let directory = tempfile::tempdir()?;
    let base = directory.path().join("document");
    fs::create_dir(&base)?;
    fs::write(directory.path().join("private.svg"), SVG)?;
    let (html, warnings) = convert(
        "image::../private.svg[Private,opts=inline]\n",
        &base,
        SafeMode::Safe,
        false,
    )?;
    assert!(!html.contains("<svg"));
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("outside"))
    );

    let (html, warnings) = convert(
        "image::../private.svg[Private,opts=inline]\n",
        &base,
        SafeMode::Unsafe,
        false,
    )?;
    assert!(html.contains("<svg"));
    assert!(warnings.is_empty(), "{warnings:?}");

    fs::File::create(base.join("oversized.svg"))?.set_len(20 * 1024 * 1024 + 1)?;
    for (target, message) in [
        ("oversized.svg", "20 MiB"),
        ("missing.svg", "could not embed image"),
    ] {
        let (html, warnings) = convert(
            &format!(":data-uri:\n\nimage::{target}[Alt]\n"),
            &base,
            SafeMode::Safe,
            false,
        )?;
        assert!(html.contains("data:image/svg+xml;base64,\""));
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.message.contains(message)),
            "{warnings:?}"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn image_embedding_confines_symlink_targets() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let base = directory.path().join("document");
    fs::create_dir(&base)?;
    fs::write(directory.path().join("private.svg"), SVG)?;
    std::os::unix::fs::symlink(directory.path().join("private.svg"), base.join("link.svg"))?;
    let (html, warnings) = convert(
        "image::link.svg[Private,opts=inline]\n",
        &base,
        SafeMode::Safe,
        false,
    )?;
    assert!(!html.contains("<svg"));
    assert_eq!(warnings.len(), 1);
    Ok(())
}

#[test]
fn remote_image_embedding_requires_permission() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let target = format!("http://{}/image.svg", listener.local_addr()?);
    let (html, warnings) = convert(
        &format!(":data-uri:\n\nimage::{target}[Alt]\n"),
        directory.path(),
        SafeMode::Safe,
        false,
    )?;
    assert!(html.contains(&target));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        listener.accept().err().map(|error| error.kind()),
        Some(std::io::ErrorKind::WouldBlock)
    );
    Ok(())
}

#[cfg(feature = "network")]
#[test]
fn remote_image_embedding_reads_authorized_bytes() -> Result<(), Error> {
    use std::io::{BufRead as _, Write as _};
    let directory = tempfile::tempdir()?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let target = format!("http://{}/asset", listener.local_addr()?);
    listener.set_nonblocking(true)?;
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        };
        stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
        let mut request = Vec::new();
        let mut reader = std::io::BufReader::new(&mut stream);
        loop {
            let count = reader.read_until(b'\n', &mut request)?;
            if count == 0 || request.len() > 4096 {
                return Err(std::io::Error::other("invalid HTTP request"));
            }
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: image/svg+xml; charset=utf-8\r\nConnection: close\r\n\r\n{SVG}",
            SVG.len()
        )
    });
    let (html, warnings) = convert(
        &format!(":data-uri:\n\nimage::{target}[Alt]\n"),
        directory.path(),
        SafeMode::Safe,
        true,
    )?;
    server.join().map_err(|_| "HTTP server failed")??;
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(html.contains(&format!(
        "data:image/svg+xml;base64,{}",
        STANDARD.encode(SVG)
    )));
    Ok(())
}

#[cfg(not(feature = "network"))]
#[test]
fn remote_image_embedding_without_network_reports_a_warning() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let (_, warnings) = convert(
        ":data-uri:\n\nimage::https://example.test/image.svg[Alt]\n",
        directory.path(),
        SafeMode::Safe,
        true,
    )?;
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("network feature"))
    );
    Ok(())
}
