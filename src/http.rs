use anyhow::bail;
use memchr::memchr;
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls::pki_types::ServerName;
use std::sync::Arc;
use std::sync::LazyLock;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use url::ParseError;
use url::Url;

static TLS_CONFIG: LazyLock<Arc<rustls::ClientConfig>> = LazyLock::new(|| {
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();
    Arc::new(config)
});

pub fn init_tls_certs() {
    LazyLock::force(&TLS_CONFIG);
}

pub async fn request_document(url_str: &str) -> anyhow::Result<Document> {
    const MAX_REDIRECTS: usize = 5;

    let mut url = Url::parse(url_str)?;

    for _ in 0..MAX_REDIRECTS {
        let scheme = match url.scheme() {
            "http" => Scheme::Http,
            "https" => Scheme::Https,
            _ => bail!("only http(s) supported"),
        };

        let port = url.port_or_known_default().unwrap();
        let host = url.host_str().unwrap();

        let tcp_stream = TcpStream::connect((host, port)).await?;
        tcp_stream.set_nodelay(true)?;

        let response = match scheme {
            Scheme::Http => http_get(Stream::Plain(tcp_stream), url).await?,
            Scheme::Https => {
                let domain = ServerName::try_from(host).unwrap().to_owned();
                let connector = TlsConnector::from(TLS_CONFIG.clone());
                let tls_stream = connector.connect(domain, tcp_stream).await?;
                http_get(Stream::Tls(Box::new(tls_stream)), url).await?
            }
        };

        match response {
            HttpResponse::Ok(document) => {
                return Ok(document);
            }
            HttpResponse::Redirect(redirect_url) => {
                url = redirect_url;
            }
        }
    }

    bail!("too many redirects")
}

pub enum Document {
    Unsupported(Url),
    Html(Url, Box<HtmlBodyReader>),
    Pdf(Url),
}

pub struct HtmlBodyReader {
    stream: Stream,
    buffer: Vec<u8>,
}

impl HtmlBodyReader {
    pub async fn extract_title(&mut self) -> anyhow::Result<String> {
        const PREFIX: &str = "title";

        assert!(self.buffer.capacity() >= PREFIX.len());

        enum State {
            Start,
            Name,
            Attributes,
            Value(Vec<u8>),
        }

        let mut state = State::Start;

        loop {
            println!("loop");
            match &mut state {
                State::Start => match memchr(b'<', self.buffer.as_slice()) {
                    Some(tag_start) => {
                        let _ = self.buffer.drain(..=tag_start);
                        state = State::Name;
                        continue;
                    }
                    None => {
                        let _ = self.buffer.drain(..self.buffer.len() - PREFIX.len());
                    }
                },
                State::Name => {
                    if self.buffer.len() >= PREFIX.len() {
                        let tag_name = &self.buffer.as_slice()[..PREFIX.len()];

                        let tag_found = PREFIX.eq_ignore_ascii_case(str::from_utf8(tag_name)?);

                        if tag_found {
                            state = State::Attributes;
                            continue;
                        } else {
                            state = State::Start;
                        }
                    }
                }
                State::Attributes => match memchr(b'>', self.buffer.as_slice()) {
                    Some(tag_end) => {
                        let _ = self.buffer.drain(..=tag_end);
                        state = State::Value(Vec::new());
                    }
                    None => {
                        self.buffer.clear();
                    }
                },
                State::Value(title) => {
                    if let Some(tag_start) = memchr(b'<', self.buffer.as_slice()) {
                        title.extend_from_slice(&self.buffer.as_slice()[..tag_start]);

                        break;
                    }
                    title.extend_from_slice(self.buffer.as_slice());
                    self.buffer.clear();
                }
            }

            println!("reading more");
            self.stream.read_buf(&mut self.buffer).await?;
        }

        println!("exit loop");

        let State::Value(title) = state else {
            unreachable!();
        };

        let title = String::from_utf8(title)?;
        Ok(title)
    }
}

#[derive(Debug)]
enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl Stream {
    async fn write_all(&mut self, buf: &[u8]) -> tokio::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.write_all(buf).await,
            Self::Tls(stream) => stream.write_all(buf).await,
        }
    }

    async fn read_buf(&mut self, buf: &mut Vec<u8>) -> tokio::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read_buf(buf).await,
            Self::Tls(stream) => stream.read_buf(buf).await,
        }
    }
}

enum Scheme {
    Http,
    Https,
}

enum HttpResponse {
    Ok(Document),
    Redirect(Url),
}

async fn http_get(mut stream: Stream, url: Url) -> anyhow::Result<HttpResponse> {
    const HTTP_BUFFER_SIZE: usize = 4 * 1024;

    enum ExpectedHeader {
        ContentType,
        Location,
    }

    let mut request = String::with_capacity(HTTP_BUFFER_SIZE);
    request.push_str("GET ");
    request.push_str(url.path());
    request.push_str(" HTTP/1.1\r\nHost: ");
    request.push_str(url.host_str().unwrap());
    request.push_str("\r\nConnection: close\r\nAccept-Encoding: \r\nAccept: text/html,application/xhtml+xml,application/pdf,*/*;q=0\r\nUser-Agent: paket\r\n\r\n");

    stream.write_all(request.as_bytes()).await?; // yolo

    let mut buffer = request.into_bytes();
    buffer.clear();

    let mut lines = LineReader::new(buffer, stream);

    // read status line
    let status_line = lines.next_line().await?;
    let mut status_line_parts = status_line.split(' ');

    if status_line_parts.next() != Some("HTTP/1.1") {
        bail!("http/1.1 expected")
    }

    let Some(status) = status_line_parts.next() else {
        bail!("no status")
    };

    let expected_header = match status {
        "200" | "203" => ExpectedHeader::ContentType,
        "300" | "301" | "302" | "303" | "307" | "308" => ExpectedHeader::Location,
        _ => bail!("unexpected status"),
    };

    // read a header
    loop {
        let line = lines.next_line().await?;
        if line.is_empty() {
            bail!("no expected header")
        }

        let mut header_parts = line.split(": ");

        let Some(header_name) = header_parts.next() else {
            bail!("invalid header")
        };

        let header_found = match expected_header {
            ExpectedHeader::ContentType => {
                header_name == "Content-Type" || header_name == "content-type"
            }
            ExpectedHeader::Location => header_name == "Location" || header_name == "location",
        };

        if header_found {
            let Some(header_value) = header_parts.next() else {
                bail!("invalid header")
            };

            match expected_header {
                ExpectedHeader::Location => {
                    let url = match Url::parse(header_value) {
                        Err(ParseError::RelativeUrlWithoutBase) => url.join(header_value),
                        anything_else => anything_else,
                    };
                    let url = url?;

                    return Ok(HttpResponse::Redirect(url));
                }
                ExpectedHeader::ContentType => {
                    let media_type = header_value.split(';').next().unwrap();
                    let document = match media_type {
                        "text/html"
                        | "TEXT/HTML"
                        | "application/xhtml+xml"
                        | "APPLICATION/XHTML+XML" => {
                            let http_body_reader = HtmlBodyReader {
                                stream: lines.stream,
                                buffer: lines.buffer,
                            };
                            Document::Html(url, Box::new(http_body_reader))
                        }
                        "application/pdf" | "APPLICATION/PDF" => Document::Pdf(url),
                        _ => Document::Unsupported(url),
                    };

                    return Ok(HttpResponse::Ok(document));
                }
            }
        }
    }
}

struct LineReader {
    buffer: Vec<u8>,
    stream: Stream,
    offset: usize,
}

impl LineReader {
    fn new(buffer: Vec<u8>, stream: Stream) -> Self {
        Self {
            buffer,
            stream,
            offset: 0,
        }
    }
}

impl LineReader {
    async fn next_line(&mut self) -> anyhow::Result<&str> {
        if self.buffer.is_empty() {
            self.read_more().await?;
        }

        loop {
            match memchr(b'\n', &self.buffer[self.offset..]) {
                Some(line_end) => {
                    let line_end = line_end.saturating_sub(1); // \r
                    let line = &self.buffer[self.offset..(self.offset + line_end)];
                    self.offset += line.len() + 2;

                    let line = str::from_utf8(line)?;
                    return Ok(line);
                }
                None => {
                    self.buffer.drain(self.offset..);
                    self.offset = 0;
                    self.read_more().await?;
                }
            }
        }
    }

    async fn read_more(&mut self) -> anyhow::Result<()> {
        if self.stream.read_buf(&mut self.buffer).await.unwrap() == 0 {
            bail!("no data")
        }
        Ok(())
    }
}
