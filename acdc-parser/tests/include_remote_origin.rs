#![cfg(feature = "network")]

use std::{
    error::Error,
    fs,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
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
            "acdc-parser-remote-origin-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let path = directory.join("main.adoc");
        fs::write(&path, source)?;
        Ok(Self { directory, path })
    }

    fn write(&self, name: &str, content: &str) -> io::Result<PathBuf> {
        let path = self.directory.join(name);
        fs::write(&path, content)?;
        Ok(path)
    }
}

impl Drop for TempDocument {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct NestedServer {
    uri: String,
    stop: mpsc::Sender<()>,
    handle: Option<JoinHandle<io::Result<Vec<String>>>>,
}

impl NestedServer {
    fn start(nested_target: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let uri = format!("http://{address}/base/parent.adoc");
        let parent_body =
            format!("Parent root start\n\ninclude::{nested_target}[]\n\nParent root end");
        let (stop, stopped) = mpsc::channel();
        let handle = thread::spawn(move || {
            let mut paths = Vec::new();
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let path = read_request_path(&mut stream)?;
                        let body = if path == "/base/parent.adoc" {
                            parent_body.clone()
                        } else {
                            format!("Nested response for {path}")
                        };
                        write_response(&mut stream, &body)?;
                        paths.push(path);
                        if paths.len() == 2 {
                            return Ok(paths);
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        match stopped.try_recv() {
                            Ok(()) | Err(TryRecvError::Disconnected) => return Ok(paths),
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

    fn finish(mut self) -> Result<Vec<String>, Box<dyn Error>> {
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

impl Drop for NestedServer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_request_path(stream: &mut TcpStream) -> io::Result<String> {
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let bytes = buffer
            .get(..read)
            .ok_or_else(|| io::Error::other("socket read exceeded buffer"))?;
        request.extend_from_slice(bytes);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    let request = std::str::from_utf8(&request)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(str::to_string)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request path"))
}

fn write_response(stream: &mut TcpStream, body: &str) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())
}

fn options() -> Options<'static> {
    Options::builder()
        .with_safe_mode(SafeMode::Server)
        .with_attribute("allow-uri-read", true)
        .build()
}

fn paragraph_texts(result: &ParseResult) -> Result<Vec<&str>, Box<dyn Error>> {
    result
        .document()
        .blocks
        .iter()
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return Err(format!("expected paragraph, got {block:?}").into());
            };
            let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
                return Err(format!("expected plain paragraph text, got {paragraph:?}").into());
            };
            Ok(text.content)
        })
        .collect()
}

fn assert_nested_include(
    nested_target: String,
    expected_path: String,
    document: Option<TempDocument>,
) -> TestResult {
    let server = NestedServer::start(&nested_target)?;
    let document = document.map_or_else(|| TempDocument::new(""), Ok)?;
    fs::write(&document.path, format!("include::{}[]", server.uri))?;
    let parent_uri = server.uri.clone();

    let result = parse_file(&document.path, &options())?;
    let expected_nested = format!("Nested response for {expected_path}");

    assert_eq!(
        paragraph_texts(&result)?,
        [
            "Parent root start",
            expected_nested.as_str(),
            "Parent root end",
        ]
    );
    assert!(result.warnings().is_empty());
    assert_eq!(
        server.finish()?,
        ["/base/parent.adoc".to_string(), expected_path]
    );

    let [_, Block::Paragraph(nested), _] = result.document().blocks.as_slice() else {
        return Err("expected three paragraphs".into());
    };
    let expected_chain = vec![parent_uri, nested_target];
    assert_eq!(nested.location.start.file.as_deref(), Some(&expected_chain));

    Ok(())
}

#[test]
fn nested_remote_targets_preserve_asciidoctor_uri_construction() -> TestResult {
    for (target, expected_path) in [
        ("child.adoc", "/base/child.adoc"),
        ("/child.adoc", "/base//child.adoc"),
        ("../child.adoc", "/base/../child.adoc"),
    ] {
        assert_nested_include(target.to_string(), expected_path.to_string(), None)?;
    }
    Ok(())
}

#[test]
fn nested_remote_absolute_path_cannot_pivot_to_local_file() -> TestResult {
    let document = TempDocument::new("")?;
    let sentinel = document.write("sentinel.adoc", "LOCAL SENTINEL MUST NOT BE READ")?;
    let target = sentinel.to_string_lossy().into_owned();
    let expected_path = format!("/base/{target}");

    assert_nested_include(target, expected_path, Some(document))
}
