#![cfg(feature = "network")]

use std::{
    error::Error,
    fs,
    io::{self, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use acdc_parser::{Block, InlineNode, Options, ParseResult, SafeMode, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDocument {
    directory: PathBuf,
    path: PathBuf,
}

impl TempDocument {
    fn new(source: &str) -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "acdc-parser-uri-authority-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let path = directory.join("main.adoc");
        fs::write(&path, source)?;
        Ok(Self { directory, path })
    }
}

impl Drop for TempDocument {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct TestServer {
    uri: String,
    downloaded_path: PathBuf,
    stop: mpsc::Sender<()>,
    handle: Option<JoinHandle<io::Result<bool>>>,
}

impl TestServer {
    fn start(body: &'static str) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let file_name = format!("remote-{}.adoc", address.port());
        let uri = format!("http://{address}/{file_name}");
        let downloaded_path = std::env::temp_dir().join(file_name);
        let (stop, stopped) = mpsc::channel();
        let handle = thread::spawn(move || {
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes())?;
                        return Ok(true);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        match stopped.try_recv() {
                            Ok(()) | Err(TryRecvError::Disconnected) => return Ok(false),
                            Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(5)),
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
        });

        Ok(Self {
            uri,
            downloaded_path,
            stop,
            handle: Some(handle),
        })
    }

    fn finish(mut self) -> Result<bool, Box<dyn Error>> {
        let _ = self.stop.send(());
        let handle = self
            .handle
            .take()
            .ok_or_else(|| io::Error::other("test server already finished"))?;
        handle
            .join()
            .map_err(|_| io::Error::other("test server thread failed"))?
            .map_err(Into::into)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        let _ = fs::remove_file(&self.downloaded_path);
    }
}

fn assert_single_paragraph(result: &ParseResult, expected: &str) -> TestResult {
    let [Block::Paragraph(paragraph)] = result.document().blocks.as_slice() else {
        return Err(format!("expected one paragraph, got {:?}", result.document().blocks).into());
    };
    let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
        return Err(format!("expected plain paragraph text, got {paragraph:?}").into());
    };
    assert_eq!(text.content, expected);
    Ok(())
}

#[test]
fn document_attribute_cannot_grant_uri_read_authority() -> TestResult {
    let server = TestServer::start("Remote content.")?;
    let document = TempDocument::new(&format!(":allow-uri-read:\n\ninclude::{}[]", server.uri))?;

    let options = Options::builder().with_safe_mode(SafeMode::Server).build();
    let _result = parse_file(&document.path, &options)?;

    // The denied-include fallback and diagnostic remain a separate compatibility
    // slice; this test locks only the caller-authority boundary.
    assert!(!server.finish()?);

    Ok(())
}

#[test]
fn caller_attribute_presence_grants_uri_read_authority() -> TestResult {
    for value in ["", "false"] {
        let server = TestServer::start("Remote content.")?;
        let document = TempDocument::new(&format!("include::{}[]", server.uri))?;
        let options = Options::builder()
            .with_safe_mode(SafeMode::Server)
            .with_attribute("allow-uri-read", value)
            .build();

        let result = parse_file(&document.path, &options)?;

        assert_single_paragraph(&result, "Remote content.")?;
        assert!(result.warnings().is_empty());
        assert!(server.finish()?);
    }

    Ok(())
}

#[test]
fn unset_caller_values_do_not_grant_uri_read_authority() -> TestResult {
    let options = [
        Options::builder()
            .with_safe_mode(SafeMode::Server)
            .with_attribute("allow-uri-read", false)
            .build(),
        Options::builder()
            .with_safe_mode(SafeMode::Server)
            .with_attribute("allow-uri-read", ())
            .build(),
    ];

    for options in options {
        let server = TestServer::start("Remote content.")?;
        let document = TempDocument::new(&format!("include::{}[]", server.uri))?;

        let _result = parse_file(&document.path, &options)?;

        assert!(!server.finish()?);
    }

    Ok(())
}

#[test]
fn document_cannot_revoke_caller_uri_read_authority() -> TestResult {
    let server = TestServer::start("Remote content.")?;
    let document = TempDocument::new(&format!(":allow-uri-read!:\n\ninclude::{}[]", server.uri))?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Server)
        .with_attribute("allow-uri-read", true)
        .build();

    let result = parse_file(&document.path, &options)?;

    assert_single_paragraph(&result, "Remote content.")?;
    assert!(result.document().attributes.contains_key("allow-uri-read"));
    assert!(result.warnings().is_empty());
    assert!(server.finish()?);

    Ok(())
}
