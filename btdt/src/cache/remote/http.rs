use crate::util::close::Close;
use std::fmt::{Display, Formatter};
use std::io;
use std::io::{BufRead, BufReader, BufWriter, IntoInnerError, Read, Write};
use std::marker::PhantomData;
use std::net::TcpStream;
use url::Url;

const CRLF: &[u8] = b"\r\n";
const HTTP_VERSION: &str = "HTTP/1.1";

#[derive(Debug, Copy, Clone)]
enum TransferEncodingType {
    Chunked,
    FixedSize(usize),
}

struct AwaitingRequestHeaders<T: OptionTransferEncoding> {
    _transfer_encoding: PhantomData<T>,
}
struct AwaitingRequestBody<T: TransferEncoding> {
    _transfer_encoding: PhantomData<T>,
}
struct ReadResponseStatus;
struct ReadResponseHeaders;
struct ReadResponseBody;

trait State {}
impl<T: OptionTransferEncoding> State for AwaitingRequestHeaders<T> {}
impl<T: TransferEncoding> State for AwaitingRequestBody<T> {}
impl State for ReadResponseStatus {}
impl State for ReadResponseHeaders {}
impl State for ReadResponseBody {}

struct TNone;
struct TSome<T> {
    _type: PhantomData<T>,
}

trait OptionTransferEncoding {}
impl OptionTransferEncoding for TNone {}
impl<T: TransferEncoding> OptionTransferEncoding for TSome<T> {}

struct NoBodyTransferEncoding;
struct ChunkedTransferEncoding;
struct FixedSizeTransferEncoding;

trait TransferEncoding {}
impl TransferEncoding for NoBodyTransferEncoding {}
impl TransferEncoding for ChunkedTransferEncoding {}
impl TransferEncoding for FixedSizeTransferEncoding {}

pub struct HttpRequest<State> {
    stream: BufWriter<TcpStream>,
    _state: PhantomData<State>,
}

pub struct HttpResponse<State, Read = TcpStream> {
    stream: BufReader<Read>,
    transfer_encoding: Option<TransferEncodingType>,
    headers_exhausted: bool,
    is_eof: bool,
    chunk_bytes_remaining: usize,
    _state: PhantomData<State>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpStatus {
    status_line: String,
}

impl HttpStatus {
    fn new(status_line: String) -> Result<HttpStatus> {
        if !status_line.starts_with(HTTP_VERSION) {
            return Err(HttpClientError::invalid_data("unsupported HTTP version"));
        }
        if status_line.as_bytes()[HTTP_VERSION.len()] != b' '
            || status_line.as_bytes()[HTTP_VERSION.len() + 4] != b' '
        {
            return Err(HttpClientError::invalid_data("malformed status line"));
        }
        let status = Self { status_line };
        if status.code().as_bytes().iter().any(|c| !c.is_ascii_digit()) {
            return Err(HttpClientError::invalid_data("invalid HTTP status code"));
        }
        Ok(status)
    }

    pub fn code(&self) -> &str {
        &self.status_line[HTTP_VERSION.len() + 1..HTTP_VERSION.len() + 4]
    }

    pub fn code_u16(&self) -> u16 {
        self.code().parse().expect("invalid HTTP staus code")
    }

    pub fn is_success(&self) -> bool {
        self.code().as_bytes()[0] == b'2'
    }

    pub fn reason(&self) -> &str {
        self.status_line[HTTP_VERSION.len() + 5..].trim_end()
    }
}

#[derive(Debug)]
pub enum HttpClientError {
    InvalidScheme(String),
    MissingHost,
    UnsupportedFeature(&'static str),
    IoError(io::Error),
}

impl HttpClientError {
    pub fn invalid_data(msg: &str) -> Self {
        HttpClientError::IoError(io::Error::new(io::ErrorKind::InvalidData, msg))
    }
}

impl Display for HttpClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScheme(scheme) => write!(f, "unknown URL scheme: {}", scheme),
            Self::MissingHost => write!(f, "missing host"),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {}", feature),
            Self::IoError(err) => write!(f, "I/O error: {}", err),
        }
    }
}

impl std::error::Error for HttpClientError {}

impl From<io::Error> for HttpClientError {
    fn from(err: io::Error) -> Self {
        HttpClientError::IoError(err)
    }
}

type Result<T> = std::result::Result<T, HttpClientError>;

impl HttpRequest<AwaitingRequestHeaders<TNone>> {
    pub fn method(method: &str, url: &Url) -> Result<Self> {
        let mut stream = Self::connect(url)?;
        stream.write_all(method.as_bytes())?;
        stream.write_all(b" ")?;
        stream.write_all(url.path().as_bytes())?;
        if let Some(query) = url.query() {
            stream.write_all(b"?")?;
            stream.write_all(query.as_bytes())?;
        }
        stream.write_all(b" ")?;
        stream.write_all(HTTP_VERSION.as_bytes())?;
        stream.write_all(CRLF)?;

        let mut client = Self {
            stream,
            _state: PhantomData,
        };

        client.header("Host", url.host_str().ok_or(HttpClientError::MissingHost)?)?;
        client.header("Connection", "close")?;
        client.header("User-Agent", concat!("btdt/", env!("CARGO_PKG_VERSION")))?;

        Ok(client)
    }

    pub fn get(
        url: &Url,
    ) -> Result<HttpRequest<AwaitingRequestHeaders<TSome<NoBodyTransferEncoding>>>> {
        let client = Self::method("GET", url)?;
        Ok(HttpRequest {
            stream: client.stream,
            _state: PhantomData,
        })
    }

    pub fn post(url: &Url) -> Result<Self> {
        Self::method("POST", url)
    }

    fn connect(url: &Url) -> Result<BufWriter<TcpStream>> {
        let use_tls = match url.scheme() {
            "http" => false,
            "https" => true,
            scheme => Err(HttpClientError::InvalidScheme(scheme.into()))?,
        };
        if use_tls {
            todo!("TLS support not implemented");
        }
        if url.username() != "" || url.password().is_some() {
            return Err(HttpClientError::UnsupportedFeature(
                "username/password in URL",
            ));
        }
        let port = url.port_or_known_default().expect("default port not known");
        Ok(BufWriter::new(TcpStream::connect((
            url.host_str().ok_or(HttpClientError::MissingHost)?,
            port,
        ))?))
    }
}

impl<T: OptionTransferEncoding> HttpRequest<AwaitingRequestHeaders<T>> {
    pub fn header(&mut self, key: &str, value: &str) -> Result<()> {
        self.stream.write_all(key.as_bytes())?;
        self.stream.write_all(b": ")?;
        self.stream.write_all(value.as_bytes())?;
        self.stream.write_all(CRLF)?;
        Ok(())
    }

    pub fn no_body(mut self) -> Result<HttpResponse<ReadResponseStatus>> {
        self.stream.write_all(CRLF)?;
        Ok(HttpResponse {
            stream: BufReader::new(
                self.stream
                    .into_inner()
                    .map_err(IntoInnerError::into_error)?,
            ),
            transfer_encoding: None,
            is_eof: false,
            headers_exhausted: false,
            chunk_bytes_remaining: 0,
            _state: PhantomData,
        })
    }
}

impl HttpRequest<AwaitingRequestHeaders<TNone>> {
    pub fn body_with_size(
        mut self,
        size: usize,
    ) -> Result<HttpRequest<AwaitingRequestBody<FixedSizeTransferEncoding>>> {
        self.header("Content-Length", &size.to_string())?;
        self.stream.write_all(CRLF)?;
        Ok(HttpRequest {
            stream: self.stream,
            _state: PhantomData,
        })
    }
}

impl Write for HttpRequest<AwaitingRequestBody<FixedSizeTransferEncoding>> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl<T: TransferEncoding> HttpRequest<AwaitingRequestBody<T>> {
    pub fn response(self) -> Result<HttpResponse<ReadResponseStatus>> {
        Ok(HttpResponse {
            stream: BufReader::new(
                self.stream
                    .into_inner()
                    .map_err(IntoInnerError::into_error)?,
            ),
            transfer_encoding: None,
            headers_exhausted: false,
            is_eof: false,
            chunk_bytes_remaining: 0,
            _state: PhantomData,
        })
    }
}

impl<S, R: Read> HttpResponse<S, R> {
    pub fn into_inner_stream(self) -> BufReader<R> {
        self.stream
    }
}

impl HttpResponse<ReadResponseStatus> {
    pub fn read_status(mut self) -> Result<(HttpStatus, HttpResponse<ReadResponseHeaders>)> {
        let mut status_line = String::new();
        self.stream.read_line(&mut status_line)?;
        let status = HttpStatus::new(status_line.trim_end().to_string())?;

        Ok((
            status,
            HttpResponse {
                stream: self.stream,
                transfer_encoding: self.transfer_encoding,
                headers_exhausted: self.headers_exhausted,
                is_eof: self.is_eof,
                chunk_bytes_remaining: self.chunk_bytes_remaining,
                _state: PhantomData,
            },
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Header {
    header_line: String,
    key_end: usize,
    value_start: usize,
    value_end: usize,
}

impl Header {
    fn new(header_line: String) -> Result<Header> {
        let key_end = header_line.find(':').ok_or_else(|| {
            HttpClientError::invalid_data("malformed header: missing colon separator")
        })?;
        let value_start = header_line[key_end + 1..]
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(0)
            + key_end
            + 1;
        let value_end = header_line
            .rfind(|c: char| !c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(header_line.len())
            .max(value_start);
        Ok(Header {
            header_line,
            key_end,
            value_start,
            value_end,
        })
    }

    pub fn key(&self) -> &str {
        &self.header_line[..self.key_end]
    }

    pub fn value(&self) -> &str {
        &self.header_line[self.value_start..self.value_end]
    }
}

impl<R: Read> HttpResponse<ReadResponseHeaders, R> {
    pub fn read_next_header(&mut self) -> Result<Option<Header>> {
        if self.headers_exhausted {
            return Ok(None);
        }
        let mut line = String::new();
        self.stream.read_line(&mut line)?;
        if line.trim().is_empty() {
            self.headers_exhausted = true;
            return Ok(None);
        }
        let header = Header::new(line)?;
        if header.key().eq_ignore_ascii_case("Transfer-Encoding") {
            if header.value().eq_ignore_ascii_case("chunked") {
                self.transfer_encoding = Some(TransferEncodingType::Chunked);
            } else {
                return Err(HttpClientError::UnsupportedFeature("transfer encoding"));
            }
        } else if header.key().eq_ignore_ascii_case("Content-Length") {
            let size: usize = header.value().parse().map_err(|_| {
                HttpClientError::invalid_data("invalid Content-Length header value")
            })?;
            self.transfer_encoding = Some(TransferEncodingType::FixedSize(size));
        }
        Ok(Some(header))
    }

    pub fn read_body(mut self) -> Result<HttpResponse<ReadResponseBody, R>> {
        while !self.headers_exhausted {
            self.read_next_header()?;
        }
        Ok(HttpResponse {
            stream: self.stream,
            transfer_encoding: self.transfer_encoding,
            headers_exhausted: true,
            is_eof: false,
            chunk_bytes_remaining: match self.transfer_encoding {
                None => 0,
                Some(TransferEncodingType::Chunked) => 0,
                Some(TransferEncodingType::FixedSize(size)) => size,
            },
            _state: PhantomData,
        })
    }
}

impl<R: Read> Iterator for HttpResponse<ReadResponseHeaders, R> {
    type Item = Result<Header>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_next_header() {
            Ok(None) => None,
            Ok(Some(header)) => Some(Ok(header)),
            Err(err) => Some(Err(err)),
        }
    }
}

impl<R: Read> Read for HttpResponse<ReadResponseBody, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.is_eof {
            return Ok(0);
        }
        match self.transfer_encoding {
            None => {
                self.is_eof = true;
                Ok(0)
            }
            Some(TransferEncodingType::FixedSize(_)) => {
                let max_n = buf.len().min(self.chunk_bytes_remaining);
                self.stream.read(buf[..max_n].as_mut()).inspect(|n| {
                    self.chunk_bytes_remaining -= n;
                    if self.chunk_bytes_remaining == 0 {
                        self.is_eof = true;
                    }
                })
            }
            Some(TransferEncodingType::Chunked) => {
                if self.chunk_bytes_remaining == 0 {
                    let mut octets = String::new();
                    self.stream.read_line(&mut octets)?;
                    self.chunk_bytes_remaining =
                        usize::from_str_radix(octets.trim(), 16).map_err(|_| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("invalid chunk size: {}", octets.trim()),
                            )
                        })?;
                    if self.chunk_bytes_remaining == 0 {
                        self.stream.read([0; 2].as_mut())?; // trailing CRLF
                        self.is_eof = true;
                        return Ok(0);
                    }
                }
                let max_n = buf.len().min(self.chunk_bytes_remaining);
                let n = self.stream.read(&mut buf[..max_n]).inspect(|n| {
                    self.chunk_bytes_remaining -= n;
                })?;
                self.stream.read_exact([0; 2].as_mut())?; // trailing CRLF
                Ok(n)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::net::{SocketAddr, TcpListener};
    use std::ops::{Deref, DerefMut};
    use std::thread;
    use std::thread::JoinHandle;

    const EMPTY_RESPONSE: &str = "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n";

    struct TestServer {
        join_handle: JoinHandle<io::Result<String>>,
        addr: SocketAddr,
    }

    impl TestServer {
        pub fn start(response: String) -> io::Result<Self> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let addr = listener.local_addr()?;
            let join_handle = thread::spawn(move || Self::serve_once(listener, &response));
            Ok(Self { join_handle, addr })
        }

        fn serve_once(listener: TcpListener, response: &str) -> io::Result<String> {
            let (mut stream, _) = listener.accept()?;
            let mut stream = BufReader::new(&mut stream);

            let mut request_line = String::new();
            stream.read_line(&mut request_line)?;
            let mut lines: Vec<String> = vec![request_line];
            let mut reader: HttpResponse<ReadResponseHeaders, _> = HttpResponse {
                stream,
                transfer_encoding: None,
                headers_exhausted: false,
                is_eof: false,
                chunk_bytes_remaining: 0,
                _state: Default::default(),
            };
            while let Some(header) = reader
                .read_next_header()
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?
            {
                lines.push(header.header_line);
            }
            lines.push("\r\n".into());
            let mut body_reader = reader
                .read_body()
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            let mut body = String::new();
            body_reader.read_to_string(&mut body)?;
            lines.push(body);

            let stream = body_reader.into_inner_stream().into_inner();
            stream.write_all(response.as_bytes())?;
            Ok(lines.join(""))
        }

        pub fn request(self) -> io::Result<String> {
            self.join_handle.join().unwrap()
        }

        pub fn addr(&self) -> SocketAddr {
            self.addr
        }
    }

    #[test]
    fn test_get_request_without_body() -> Result<()> {
        let test_server = TestServer::start(EMPTY_RESPONSE.into())?;
        let addr = test_server.addr();
        let url = Url::parse(&format!(
            "http://{}:{}/path?query=foo#fragment",
            addr.ip().to_string(),
            addr.port()
        ))
        .unwrap();
        let response = HttpRequest::get(&url)?.no_body()?;

        assert_eq!(
            test_server.request()?,
            format!(
                "GET /path?query=foo HTTP/1.1\r\n\
                Host: {}\r\n\
                Connection: close\r\n\
                User-Agent: btdt/{}\r\n\r\n",
                addr.ip(),
                env!("CARGO_PKG_VERSION")
            )
        );

        let (status, mut response) = response.read_status()?;
        assert_eq!(
            status,
            HttpStatus::new("HTTP/1.1 204 No Content".to_string())?
        );
        assert_eq!(
            response.read_next_header()?,
            Some(Header::new("Content-Length: 0\r\n".to_string())?)
        );
        assert_eq!(response.read_next_header()?, None);
        let mut buf = String::new();
        response.read_body().unwrap().read_to_string(&mut buf)?;
        assert!(buf.is_empty());
        Ok(())
    }

    #[test]
    fn test_post_with_fixed_size_body() -> Result<()> {
        let test_server = TestServer::start(EMPTY_RESPONSE.into())?;
        let addr = test_server.addr();
        let url = Url::parse(&format!(
            "http://{}:{}/path?query=foo#fragment",
            addr.ip().to_string(),
            addr.port()
        ))
        .unwrap();
        let body = "{\"hello\": \"world\"}\r\n";
        let mut request = HttpRequest::post(&url)?.body_with_size(body.len())?;
        request.write_all(body.as_bytes())?;
        let response = request.response()?;

        assert_eq!(
            test_server.request()?,
            format!(
                "POST /path?query=foo HTTP/1.1\r\n\
                Host: {}\r\n\
                Connection: close\r\n\
                User-Agent: btdt/{}\r\n\
                Content-Length: {}\r\n\r\n\
                {}",
                addr.ip(),
                env!("CARGO_PKG_VERSION"),
                body.len(),
                body
            )
        );

        let (status, mut response) = response.read_status()?;
        assert_eq!(
            status,
            HttpStatus::new("HTTP/1.1 204 No Content".to_string())?
        );
        assert_eq!(
            response.read_next_header()?,
            Some(Header::new("Content-Length: 0\r\n".to_string())?)
        );
        assert_eq!(response.read_next_header()?, None);
        let mut buf = String::new();
        response.read_body().unwrap().read_to_string(&mut buf)?;
        assert!(buf.is_empty());
        Ok(())
    }

    #[test]
    fn test_response_body_with_content_length() -> Result<()> {
        let test_server = TestServer::start(
            "\
            HTTP/1.1 200 OK\r\n\
            Content-Length: 8\r\n\
            \r\n\
            Hello!\r\n"
                .into(),
        )?;
        let addr = test_server.addr();
        let url = Url::parse(&format!(
            "http://{}:{}/path?query=foo#fragment",
            addr.ip().to_string(),
            addr.port()
        ))
        .unwrap();
        let response = HttpRequest::get(&url)?.no_body()?;

        let (status, response) = response.read_status()?;
        assert_eq!(status, HttpStatus::new("HTTP/1.1 200 OK".to_string())?);
        let mut buf = String::new();
        response.read_body().unwrap().read_to_string(&mut buf)?;
        assert_eq!(&buf, "Hello!\r\n");
        Ok(())
    }

    #[test]
    fn test_response_body_with_chunked_transfer_encoding() -> Result<()> {
        let test_server = TestServer::start(
            "\
            HTTP/1.1 200 OK\r\n\
            Transfer-Encoding: chunked\r\n\
            \r\n\
            a\r\nHello, wor\r\n\
            5\r\nld!\r\n\r\n\
            0\r\n\r\n"
                .into(),
        )?;
        let addr = test_server.addr();
        let url = Url::parse(&format!(
            "http://{}:{}/path?query=foo#fragment",
            addr.ip().to_string(),
            addr.port()
        ))
        .unwrap();
        let response = HttpRequest::get(&url)?.no_body()?;

        let (status, response) = response.read_status()?;
        assert_eq!(status, HttpStatus::new("HTTP/1.1 200 OK".to_string())?);
        let mut buf = String::new();
        response.read_body().unwrap().read_to_string(&mut buf)?;
        assert_eq!(&buf, "Hello, world!\r\n");
        Ok(())
    }
}
