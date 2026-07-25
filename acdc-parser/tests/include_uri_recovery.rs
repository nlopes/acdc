#![cfg(feature = "network")]

use std::{
    error::Error,
    fs, io,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use acdc_parser::{Block, InlineNode, Options, ParseResult, SafeMode, parse_file};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDocument {
    directory: PathBuf,
    path: PathBuf,
}

impl TempDocument {
    fn new(source: &str) -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "acdc-parser-uri-recovery-{}-{sequence}",
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

struct StatusServer {
    uri: String,
    stop: mpsc::Sender<()>,
    handle: Option<JoinHandle<io::Result<bool>>>,
}

impl StatusServer {
    fn start(status: &'static str) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let uri = format!("http://{address}/missing.adoc");
        let (stop, stopped) = mpsc::channel();
        let handle = thread::spawn(move || {
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        use std::io::Write;

                        let response = format!(
                            "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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
            stop,
            handle: Some(handle),
        })
    }

    fn finish(mut self) -> TestResult {
        let _ = self.stop.send(());
        let handle = self
            .handle
            .take()
            .ok_or_else(|| io::Error::other("test server already finished"))?;
        let requested = handle
            .join()
            .map_err(|_| io::Error::other("test server thread failed"))??;
        assert!(requested);
        Ok(())
    }
}

impl Drop for StatusServer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn refused_uri() -> io::Result<String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let address = listener.local_addr()?;
    drop(listener);
    Ok(format!("http://{address}/missing.adoc"))
}

fn parse_authorized_uri(uri: &str) -> Result<(TempDocument, ParseResult), acdc_parser::Error> {
    let document = TempDocument::new(&format!("BEFORE\n\ninclude::{uri}[lines=1]\n\nAFTER"))?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Server)
        .with_attribute("allow-uri-read", true)
        .build();
    let result = parse_file(&document.path, &options)?;
    Ok((document, result))
}

fn paragraph_texts(result: &ParseResult) -> TestResult<Vec<String>> {
    result
        .document()
        .blocks
        .iter()
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return Err(format!("expected paragraph, got {block:?}").into());
            };
            paragraph
                .content
                .iter()
                .map(|inline| {
                    let InlineNode::PlainText(text) = inline else {
                        return Err(format!("expected plain paragraph text, got {inline:?}").into());
                    };
                    Ok(text.content)
                })
                .collect::<TestResult<Vec<_>>>()
                .map(|parts| parts.concat())
        })
        .collect()
}

fn assert_uri_recovery(result: &ParseResult, uri: &str, source: &Path) -> TestResult {
    let unresolved = format!("Unresolved directive in main.adoc - include::{uri}[lines=1]");
    assert_eq!(
        paragraph_texts(result)?,
        ["BEFORE".to_string(), unresolved, "AFTER".to_string()]
    );
    let [warning] = result.warnings() else {
        return Err(format!("expected one warning, got {:?}", result.warnings()).into());
    };
    assert_eq!(
        warning.kind.to_string(),
        format!("include uri not readable: {uri}")
    );
    let Some(location) = warning.source_location() else {
        return Err("expected the warning to have a source location".into());
    };
    assert_eq!(location.file.as_deref(), Some(source));
    assert_eq!(location.location.start.line, 3);

    let Some(Block::Paragraph(fallback)) = result.document().blocks.get(1) else {
        return Err("expected the fallback paragraph".into());
    };
    assert_eq!(fallback.location.start.line, 3);
    assert!(fallback.location.start.file.is_none());
    Ok(())
}

#[test]
fn connection_failure_inserts_fallback_and_continues() -> TestResult {
    let uri = refused_uri()?;
    let (document, result) = parse_authorized_uri(&uri)?;

    assert_uri_recovery(&result, &uri, &document.path)
}

#[test]
fn http_status_failure_inserts_fallback_and_continues() -> TestResult {
    let server = StatusServer::start("404 Not Found")?;
    let (document, result) = parse_authorized_uri(&server.uri)?;

    assert_uri_recovery(&result, &server.uri, &document.path)?;
    server.finish()
}
