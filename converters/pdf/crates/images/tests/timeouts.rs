#![cfg(feature = "network")]

use std::{
    error::Error as StdError,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

use acdc_pdf_images::{Error, ResolveConfig, resolve};

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>"#;

#[derive(Clone, Copy)]
enum SlowResponse {
    Headers,
    Body,
    Stream,
}

#[test]
fn stalled_headers_time_out_without_losing_healthy_images() -> Result<(), Box<dyn StdError>> {
    assert_timeout_keeps_healthy_image(SlowResponse::Headers)
}

#[test]
fn stalled_body_keeps_timeout_error_and_healthy_images() -> Result<(), Box<dyn StdError>> {
    assert_timeout_keeps_healthy_image(SlowResponse::Body)
}

#[test]
fn streamed_bytes_do_not_restart_the_request_deadline() -> Result<(), Box<dyn StdError>> {
    assert_timeout_keeps_healthy_image(SlowResponse::Stream)
}

fn assert_timeout_keeps_healthy_image(mode: SlowResponse) -> Result<(), Box<dyn StdError>> {
    for workers in [1, 2] {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let slow = format!("http://{address}/slow.svg");
        let healthy = format!("http://{address}/healthy.svg");
        let bytes_sent = AtomicUsize::new(0);
        let root = tempfile::tempdir()?;
        let config = ResolveConfig::new(root.path(), root.path().join("spool"))
            .with_remote_concurrency(workers)
            .with_timeout(Duration::from_secs(1));

        thread::scope(|scope| -> Result<(), Box<dyn StdError>> {
            let server = scope.spawn(|| serve_requests(&listener, mode, &bytes_sent));
            let resolved = resolve(&[&slow, &healthy], &config);
            let requests = server
                .join()
                .map_err(|_| io::Error::other("test server panicked"))??;

            assert_eq!(requests.len(), 2);
            assert!(requests.iter().any(|path| path == "/slow.svg"));
            assert!(requests.iter().any(|path| path == "/healthy.svg"));
            assert_eq!(resolved.failures.len(), 1, "{:?}", resolved.failures);
            let failure = resolved.failures.first().ok_or("timeout failure missing")?;
            assert_eq!(failure.url, slow);
            assert!(matches!(failure.error, Error::Timeout), "{failure:?}");
            assert!(resolved.assets.get(&slow).is_none());
            assert_eq!(resolved.assets.images().count(), 1);
            let image = resolved
                .assets
                .get(&healthy)
                .ok_or("healthy image missing after timeout")?;
            assert_eq!(std::fs::read(&image.path)?, SVG.as_bytes());
            if matches!(mode, SlowResponse::Stream) {
                assert!(bytes_sent.load(Ordering::SeqCst) >= 2);
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn serve_requests(
    listener: &TcpListener,
    mode: SlowResponse,
    bytes_sent: &AtomicUsize,
) -> io::Result<Vec<String>> {
    thread::scope(|scope| {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut handlers = Vec::new();
        while handlers.len() < 2 {
            match listener.accept() {
                Ok((stream, _)) => {
                    handlers.push(scope.spawn(move || serve_response(stream, mode, bytes_sent)));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "expected image requests did not arrive",
                        ));
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(error),
            }
        }
        handlers
            .into_iter()
            .map(|handler| {
                handler
                    .join()
                    .map_err(|_| io::Error::other("response handler panicked"))?
            })
            .collect()
    })
}

fn serve_response(
    mut stream: TcpStream,
    mode: SlowResponse,
    bytes_sent: &AtomicUsize,
) -> io::Result<String> {
    stream.set_nonblocking(false)?;
    // Bound the test server too, so a broken client deadline cannot hang the test.
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut reader = BufReader::new(&mut stream);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" {
            break;
        }
    }
    drop(reader);
    let path = request
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::other("request path missing"))?
        .to_string();

    if path == "/slow.svg" && matches!(mode, SlowResponse::Headers) {
        let _ = stream.read(&mut [0]);
        return Ok(path);
    }
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        SVG.len()
    )?;
    if path == "/healthy.svg" {
        stream.write_all(SVG.as_bytes())?;
    } else if matches!(mode, SlowResponse::Body) {
        stream.write_all(b"<svg")?;
        let _ = stream.read(&mut [0]);
    } else {
        // Keep sending bytes within the idle interval, but finish after the
        // client's deadline. An idle timeout alone would accept this image.
        for byte in SVG.as_bytes() {
            if let Err(error) = stream.write_all(std::slice::from_ref(byte)) {
                if matches!(
                    error.kind(),
                    io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                ) {
                    break;
                }
                return Err(error);
            }
            bytes_sent.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(50));
        }
    }
    Ok(path)
}
