use anyhow::bail;
use memchr::memchr;
use pin_project::pin_project;
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls::pki_types::ServerName;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::LazyLock;
use std::task::Context;
use std::task::Poll;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::io::ReadBuf;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use url::ParseError;
use url::Url;

const HTTP_BUFFER_SIZE: usize = 4 * 1024;
const HTML_TITLE_TAG: &str = "title";

const _: () = const {
    assert!(HTTP_BUFFER_SIZE >= HTML_TITLE_TAG.len());
};

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

pub async fn request_document(url_str: &str) -> anyhow::Result<Document<PlainOrTls>> {
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
            Scheme::Http => http_get(PlainOrTls::Plain(tcp_stream), url).await?,
            Scheme::Https => {
                let domain = ServerName::try_from(host).unwrap().to_owned();
                let connector = TlsConnector::from(TLS_CONFIG.clone());
                let tls_stream = connector.connect(domain, tcp_stream).await?;
                http_get(PlainOrTls::Tls(Box::new(tls_stream)), url).await?
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

pub enum Document<S> {
    Unsupported(Url),
    Html(Url, HtmlBodyReader<S>),
    Pdf(Url),
}

pub struct HtmlBodyReader<S> {
    stream: S,
    buffer: Vec<u8>,
}

impl<S: AsyncReadExt + Unpin> HtmlBodyReader<S> {
    fn new(stream: S, buffer: Vec<u8>) -> Self {
        Self { stream, buffer }
    }

    pub async fn extract_title(&mut self) -> anyhow::Result<Option<String>> {
        debug_assert!(self.buffer.capacity() >= HTML_TITLE_TAG.len());

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
                        let _ = self
                            .buffer
                            .drain(..self.buffer.len().saturating_sub(HTML_TITLE_TAG.len()));
                    }
                },
                State::Name => {
                    if self.buffer.len() >= HTML_TITLE_TAG.len() {
                        let tag_name = &self.buffer.as_slice()[..HTML_TITLE_TAG.len()];

                        let tag_found =
                            HTML_TITLE_TAG.eq_ignore_ascii_case(str::from_utf8(tag_name)?);

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

                        return Ok(Some(String::from_utf8_lossy(title).into_owned()));
                    }
                    title.extend_from_slice(self.buffer.as_slice());
                    self.buffer.clear();
                }
            }

            println!("reading more");
            let bytes_read = self.stream.read_buf(&mut self.buffer).await?;

            if bytes_read == 0 {
                println!("no title");
                break;
            }
        }

        Ok(None)
    }
}

#[pin_project(project = PlainOrTlsProj)]
#[derive(Debug)]
pub enum PlainOrTls {
    Plain(#[pin] TcpStream),
    Tls(#[pin] Box<TlsStream<TcpStream>>),
}

impl AsyncRead for PlainOrTls {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            PlainOrTlsProj::Plain(stream) => stream.poll_read(cx, buf),
            PlainOrTlsProj::Tls(stream) => stream.poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for PlainOrTls {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        match self.project() {
            PlainOrTlsProj::Plain(stream) => stream.poll_write(cx, buf),
            PlainOrTlsProj::Tls(stream) => stream.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            PlainOrTlsProj::Plain(stream) => stream.poll_flush(cx),
            PlainOrTlsProj::Tls(stream) => stream.poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            PlainOrTlsProj::Plain(stream) => stream.poll_shutdown(cx),
            PlainOrTlsProj::Tls(stream) => stream.poll_shutdown(cx),
        }
    }
}

enum Scheme {
    Http,
    Https,
}

enum HttpResponse<S> {
    Ok(Document<S>),
    Redirect(Url),
}

async fn http_get<S: AsyncReadExt + AsyncWriteExt + Unpin>(
    mut stream: S,
    url: Url,
) -> anyhow::Result<HttpResponse<S>> {
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
                            let http_body_reader = HtmlBodyReader::new(lines.stream, lines.buffer);
                            Document::Html(url, http_body_reader)
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

/// Cannot use `tokio::io::Lines` because it may lose data when converting back to inner
struct LineReader<S> {
    buffer: Vec<u8>,
    stream: S,
    offset: usize,
}

impl<S: AsyncReadExt + Unpin> LineReader<S> {
    fn new(buffer: Vec<u8>, stream: S) -> Self {
        Self {
            buffer,
            stream,
            offset: 0,
        }
    }

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

#[cfg(test)]
mod tests {
    use crate::http::HtmlBodyReader;

    #[tokio::test]
    async fn extract_title_case_insensitive() {
        let html = br#"
            <!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 3.2 Final//EN">
            <HTML>
                <HEAD>
                    <META NAME="foo" CONTENT="bar">
                    <tItLe>Hello Title!</tItLe>
                </HEAD>
                <BODY>
                </BODY>
            </HTML>
        "#;

        let mut body_reader = HtmlBodyReader::new(&html[..], Vec::with_capacity(64));
        let title = body_reader.extract_title().await.unwrap();

        assert_eq!(title, Some("Hello Title!".to_string()));
    }

    #[tokio::test]
    async fn extract_title_with_non_empty_buffer() {
        let html = b"<title>Read Me!</title>";

        let mut body_reader = HtmlBodyReader::new(&html[..], vec![b'F'; 8]);
        let title = body_reader.extract_title().await.unwrap();

        assert_eq!(title, Some("Read Me!".to_string()));
    }

    #[tokio::test]
    async fn extract_non_existent_title() {
        let html = br#"
            <!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 3.2 Final//EN">
            <HTML>
                <HEAD>
                    <META NAME="foo" CONTENT="bar">
                </HEAD>
                <BODY>
                </BODY>
            </HTML>
        "#;

        let mut body_reader = HtmlBodyReader::new(&html[..], Vec::with_capacity(64));
        let title = body_reader.extract_title().await.unwrap();

        assert_eq!(title, None);
    }
}
