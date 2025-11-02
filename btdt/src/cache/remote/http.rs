use crate::cache::remote::error::HttpClientError;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned, crypto};
use rustls_platform_verifier::BuilderVerifierExt;
use std::io;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::marker::PhantomData;
use std::net::TcpStream;
use std::sync::Arc;
use url::Url;

const CRLF: &[u8] = b"\r\n";
const HTTP_VERSION: &str = "HTTP/1.1";

#[derive(Debug, Copy, Clone)]
enum TransferEncodingType {
    Chunked,
    FixedSize(usize),
}

pub trait State {}
pub struct AwaitingRequestHeaders<T: OptionTransferEncoding> {
    _transfer_encoding: PhantomData<T>,
}
pub struct AwaitingRequestBody<T: TransferEncoding> {
    _transfer_encoding: PhantomData<T>,
}
pub struct ReadResponseStatus;
pub struct ReadResponseHeaders;
pub struct ReadResponseBody;
impl<T: OptionTransferEncoding> State for AwaitingRequestHeaders<T> {}
impl<T: TransferEncoding> State for AwaitingRequestBody<T> {}
impl State for ReadResponseStatus {}
impl State for ReadResponseHeaders {}
impl State for ReadResponseBody {}

pub trait TransferEncoding {}
pub struct NoBodyTransferEncoding;
pub struct ChunkedTransferEncoding;
pub struct FixedSizeTransferEncoding;
impl TransferEncoding for NoBodyTransferEncoding {}
impl TransferEncoding for ChunkedTransferEncoding {}
impl TransferEncoding for FixedSizeTransferEncoding {}

pub trait OptionTransferEncoding {}
pub struct TNone;
pub struct TSome<T> {
    _type: PhantomData<T>,
}
impl OptionTransferEncoding for TNone {}
impl<T: TransferEncoding> OptionTransferEncoding for TSome<T> {}

type Result<T> = std::result::Result<T, HttpClientError>;

pub struct HttpClient {
    tls_client_config: Arc<ClientConfig>,
}

impl HttpClient {
    pub fn new(tls_client_config: Arc<ClientConfig>) -> Self {
        Self { tls_client_config }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn default() -> std::result::Result<Self, rustls::Error> {
        Ok(Self::new(Arc::new(
            ClientConfig::builder_with_provider(Arc::new(crypto::aws_lc_rs::default_provider()))
                .with_safe_default_protocol_versions()?
                .with_platform_verifier()
                .with_no_client_auth(),
        )))
    }

    pub fn method(
        &self,
        method: &str,
        url: &Url,
    ) -> Result<HttpRequest<AwaitingRequestHeaders<TNone>>> {
        let mut stream = self.connect(url)?;
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

        let mut client = HttpRequest {
            stream,
            _state: PhantomData,
        };

        client.header("Host", url.host_str().ok_or(HttpClientError::MissingHost)?)?;
        client.header("Connection", "close")?;
        client.header("User-Agent", concat!("btdt/", env!("CARGO_PKG_VERSION")))?;

        Ok(client)
    }

    pub fn get(
        &self,
        url: &Url,
    ) -> Result<HttpRequest<AwaitingRequestHeaders<TSome<NoBodyTransferEncoding>>>> {
        let client = self.method("GET", url)?;
        Ok(HttpRequest {
            stream: client.stream,
            _state: PhantomData,
        })
    }

    #[allow(unused)]
    pub fn post(&self, url: &Url) -> Result<HttpRequest<AwaitingRequestHeaders<TNone>>> {
        self.method("POST", url)
    }

    pub fn put(&self, url: &Url) -> Result<HttpRequest<AwaitingRequestHeaders<TNone>>> {
        self.method("PUT", url)
    }

    fn connect(&self, url: &Url) -> Result<Box<dyn WriteThenRead + Send>> {
        let use_tls = match url.scheme() {
            "http" => false,
            "https" => true,
            scheme => Err(HttpClientError::InvalidScheme(scheme.into()))?,
        };
        if url.username() != "" || url.password().is_some() {
            return Err(HttpClientError::UnsupportedFeature(
                "username/password in URL",
            ));
        }
        let host = url.host_str().ok_or(HttpClientError::MissingHost)?;
        let port = url.port_or_known_default().expect("default port not known");
        let stream = TcpStream::connect((host, port))?;
        if use_tls {
            let connection = ClientConnection::new(
                self.tls_client_config.clone(),
                ServerName::try_from(host.to_string())?,
            )?;
            let tls_stream = StreamOwned::new(connection, stream);
            Ok(Box::new(BufWriter::new(tls_stream)))
        } else {
            Ok(Box::new(BufWriter::new(stream)))
        }
    }
}

trait WriteThenRead: Write {
    fn into_reader(
        self: Box<Self>,
    ) -> io::Result<HttpMessageReader<Box<dyn BufRead + Send>, ReadResponseStatus>>;
}

impl WriteThenRead for BufWriter<TcpStream> {
    fn into_reader(
        self: Box<Self>,
    ) -> io::Result<HttpMessageReader<Box<dyn BufRead + Send>, ReadResponseStatus>> {
        Ok(HttpMessageReader::new(Box::new(BufReader::new(
            self.into_inner()?,
        ))))
    }
}

impl WriteThenRead for BufWriter<StreamOwned<ClientConnection, TcpStream>> {
    fn into_reader(
        self: Box<Self>,
    ) -> io::Result<HttpMessageReader<Box<dyn BufRead + Send>, ReadResponseStatus>> {
        Ok(HttpMessageReader::new(Box::new(self.into_inner()?)))
    }
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

    pub fn as_str(&self) -> &str {
        &self.status_line
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

pub struct HttpRequest<S: State> {
    stream: Box<dyn WriteThenRead + Send>,
    _state: PhantomData<S>,
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
            inner: self.stream.into_reader()?,
        })
    }
}

impl HttpRequest<AwaitingRequestHeaders<TNone>> {
    #[allow(unused)]
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

    pub fn body(mut self) -> Result<HttpRequest<AwaitingRequestBody<ChunkedTransferEncoding>>> {
        self.header("Transfer-Encoding", "chunked")?;
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

impl Write for HttpRequest<AwaitingRequestBody<ChunkedTransferEncoding>> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let chunk_size = buf.len();
        if chunk_size > 0 {
            self.stream
                .write_all(format!("{chunk_size:X}").as_bytes())?;
            self.stream.write_all(CRLF)?;
            self.stream.write_all(buf)?;
            self.stream.write_all(CRLF)?;
        }
        Ok(chunk_size)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl HttpRequest<AwaitingRequestBody<FixedSizeTransferEncoding>> {
    #[allow(unused)]
    pub fn response(self) -> Result<HttpResponse<ReadResponseStatus>> {
        Ok(HttpResponse {
            inner: self.stream.into_reader()?,
        })
    }
}

impl HttpRequest<AwaitingRequestBody<ChunkedTransferEncoding>> {
    pub fn response(mut self) -> Result<HttpResponse<ReadResponseStatus>> {
        self.stream.write_all(b"0")?;
        self.stream.write_all(CRLF)?;
        self.stream.write_all(CRLF)?;
        Ok(HttpResponse {
            inner: self.stream.into_reader()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
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

struct HttpMessageReader<R: BufRead, S: State> {
    reader: R,
    transfer_encoding: Option<TransferEncodingType>,
    headers_exhausted: bool,
    is_eof: bool,
    chunk_bytes_remaining: usize,
    _state: PhantomData<S>,
}

impl<R: BufRead, S: State> HttpMessageReader<R, S> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            transfer_encoding: None,
            headers_exhausted: false,
            is_eof: false,
            chunk_bytes_remaining: 0,
            _state: PhantomData,
        }
    }
}

impl<R: BufRead> HttpMessageReader<R, ReadResponseStatus> {
    pub fn read_status(
        mut self,
    ) -> Result<(HttpStatus, HttpMessageReader<R, ReadResponseHeaders>)> {
        let mut status_line = String::new();
        self.reader.read_line(&mut status_line)?;
        let status = HttpStatus::new(status_line.trim_end().to_string())?;

        Ok((
            status,
            HttpMessageReader {
                reader: self.reader,
                transfer_encoding: self.transfer_encoding,
                headers_exhausted: self.headers_exhausted,
                is_eof: self.is_eof,
                chunk_bytes_remaining: self.chunk_bytes_remaining,
                _state: PhantomData,
            },
        ))
    }
}

impl<R: BufRead> HttpMessageReader<R, ReadResponseHeaders> {
    #[cfg(test)]
    fn new_skip_status_line(reader: R) -> Self {
        Self {
            reader,
            transfer_encoding: None,
            headers_exhausted: false,
            is_eof: false,
            chunk_bytes_remaining: 0,
            _state: PhantomData,
        }
    }

    pub fn read_next_header(&mut self) -> Result<Option<Header>> {
        if self.headers_exhausted {
            return Ok(None);
        }
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
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

    pub fn read_body(mut self) -> Result<HttpMessageReader<R, ReadResponseBody>> {
        while !self.headers_exhausted {
            self.read_next_header()?;
        }
        Ok(HttpMessageReader {
            reader: self.reader,
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

impl<R: BufRead> Read for HttpMessageReader<R, ReadResponseBody> {
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
                self.reader.read(buf[..max_n].as_mut()).inspect(|n| {
                    self.chunk_bytes_remaining -= n;
                    if self.chunk_bytes_remaining == 0 {
                        self.is_eof = true;
                    }
                })
            }
            Some(TransferEncodingType::Chunked) => {
                if self.chunk_bytes_remaining == 0 {
                    let mut octets = String::new();
                    self.reader.read_line(&mut octets)?;
                    self.chunk_bytes_remaining =
                        usize::from_str_radix(octets.trim(), 16).map_err(|_| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("invalid chunk size: {}", octets.trim()),
                            )
                        })?;
                    if self.chunk_bytes_remaining == 0 {
                        self.reader.read([0; 2].as_mut())?; // trailing CRLF
                        self.is_eof = true;
                        return Ok(0);
                    }
                }
                let max_n = buf.len().min(self.chunk_bytes_remaining);
                let n = self.reader.read(&mut buf[..max_n]).inspect(|n| {
                    self.chunk_bytes_remaining -= n;
                })?;
                if self.chunk_bytes_remaining == 0 {
                    self.reader.read_exact([0; 2].as_mut())?; // trailing CRLF
                }
                Ok(n)
            }
        }
    }
}

pub struct HttpResponse<S: State> {
    inner: HttpMessageReader<Box<dyn BufRead + Send>, S>,
}

impl HttpResponse<ReadResponseStatus> {
    pub fn read_status(self) -> Result<(HttpStatus, HttpResponse<ReadResponseHeaders>)> {
        let (status, inner) = self.inner.read_status()?;
        Ok((status, HttpResponse { inner }))
    }
}

impl HttpResponse<ReadResponseHeaders> {
    pub fn read_next_header(&mut self) -> Result<Option<Header>> {
        self.inner.read_next_header()
    }

    pub fn read_body(self) -> Result<HttpResponse<ReadResponseBody>> {
        Ok(HttpResponse {
            inner: self.inner.read_body()?,
        })
    }
}

impl Read for HttpResponse<ReadResponseBody> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned, crypto};
    use std::net::{SocketAddr, TcpListener};
    use std::sync::Arc;
    use std::thread;
    use std::thread::JoinHandle;

    pub static CERTIFICATE_PRIVATE_KEY: &[u8] = include_bytes!("../../../../tls/leaf.key");
    pub static CERTIFICATE_PEM: &[u8] = include_bytes!("../../../../tls/leaf.pem");

    pub const EMPTY_RESPONSE: &str = "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n";

    pub struct TestServer {
        join_handle: JoinHandle<io::Result<String>>,
        addr: SocketAddr,
        base_url: Url,
    }

    impl TestServer {
        pub fn start(response: String) -> io::Result<Self> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let addr = listener.local_addr()?;
            let base_url =
                Url::parse(&format!("http://{}:{}", addr.ip().to_string(), addr.port())).unwrap();
            let join_handle = thread::spawn(move || Self::serve_once(listener, &response, None));
            Ok(Self {
                join_handle,
                addr,
                base_url,
            })
        }

        pub fn start_with_tls(response: String) -> io::Result<Self> {
            crypto::aws_lc_rs::default_provider()
                .install_default()
                .unwrap();
            let cert = CertificateDer::from_pem_slice(CERTIFICATE_PEM).unwrap();
            let private_key = PrivateKeyDer::from_pem_slice(CERTIFICATE_PRIVATE_KEY).unwrap();
            let server_conf = ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert], private_key)
                .unwrap();

            let listener = TcpListener::bind("127.0.0.1:0")?;
            let addr = listener.local_addr()?;
            let base_url = Url::parse(&format!(
                "https://{}:{}",
                addr.ip().to_string(),
                addr.port()
            ))
            .unwrap();
            let join_handle = thread::spawn(move || {
                Self::serve_once(listener, &response, Some(Arc::new(server_conf)))
            });
            Ok(Self {
                join_handle,
                addr,
                base_url,
            })
        }

        fn serve_once(
            listener: TcpListener,
            response: &str,
            tls_conf: Option<Arc<ServerConfig>>,
        ) -> io::Result<String> {
            let (stream, _) = listener.accept()?;
            if let Some(tls_conf) = tls_conf {
                let tls_connection = ServerConnection::new(tls_conf).unwrap();
                let mut stream = StreamOwned::new(tls_connection, stream);
                let body = Self::read_request(&mut stream)?;
                stream.write_all(response.as_bytes())?;
                Ok(body)
            } else {
                let mut stream = BufReader::new(stream);
                let body = Self::read_request(&mut stream)?;
                stream.into_inner().write_all(response.as_bytes())?;
                Ok(body)
            }
        }

        fn read_request<R: BufRead>(stream: &mut R) -> io::Result<String> {
            let mut request_line = String::new();
            stream.read_line(&mut request_line)?;
            let mut lines: Vec<String> = vec![request_line];
            let mut reader = HttpMessageReader::new_skip_status_line(stream);
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
            Ok(lines.join(""))
        }

        pub fn request(self) -> io::Result<String> {
            self.join_handle.join().unwrap()
        }

        pub fn addr(&self) -> SocketAddr {
            self.addr
        }

        pub fn base_url(&self) -> &Url {
            &self.base_url
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
        let response = HttpClient::default()?.get(&url)?.no_body()?;

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
        let mut url = test_server.base_url().join("path").unwrap();
        url.query_pairs_mut().append_pair("query", "foo");
        url.set_fragment(Some("fragment"));
        let body = "{\"hello\": \"world\"}\r\n";
        let mut request = HttpClient::default()?
            .post(&url)?
            .body_with_size(body.len())?;
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
    fn test_post_with_chunked_body() -> Result<()> {
        let test_server = TestServer::start(EMPTY_RESPONSE.into())?;
        let addr = test_server.addr();
        let mut url = test_server.base_url().join("path").unwrap();
        url.query_pairs_mut().append_pair("query", "foo");
        url.set_fragment(Some("fragment"));
        let body = "{\"hello\": \"world\"}\r\n";
        let mut request = HttpClient::default()?.post(&url)?.body()?;
        request.write_all(&body.as_bytes()[..5])?;
        request.write_all(&body.as_bytes()[5..])?;
        let response = request.response()?;

        assert_eq!(
            test_server.request()?,
            format!(
                "POST /path?query=foo HTTP/1.1\r\n\
                Host: {}\r\n\
                Connection: close\r\n\
                User-Agent: btdt/{}\r\n\
                Transfer-Encoding: chunked\r\n\r\n\
                {}",
                addr.ip(),
                env!("CARGO_PKG_VERSION"),
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
        let mut url = test_server.base_url().join("path").unwrap();
        url.query_pairs_mut().append_pair("query", "foo");
        url.set_fragment(Some("fragment"));
        let response = HttpClient::default()?.get(&url)?.no_body()?;

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
        let mut url = test_server.base_url().join("path").unwrap();
        url.query_pairs_mut().append_pair("query", "foo");
        url.set_fragment(Some("fragment"));
        let response = HttpClient::default()?.get(&url)?.no_body()?;

        let (status, response) = response.read_status()?;
        assert_eq!(status, HttpStatus::new("HTTP/1.1 200 OK".to_string())?);
        let mut buf = String::new();
        response.read_body().unwrap().read_to_string(&mut buf)?;
        assert_eq!(&buf, "Hello, world!\r\n");
        Ok(())
    }

    #[test]
    fn test_tls() -> Result<()> {
        let root_cert =
            CertificateDer::from_pem_slice(include_bytes!("../../../../tls/ca.pem")).unwrap();
        let mut cert_store = RootCertStore::empty();
        cert_store.add(root_cert)?;
        let tls_client_config = Arc::new(
            ClientConfig::builder_with_provider(Arc::new(crypto::aws_lc_rs::default_provider()))
                .with_safe_default_protocol_versions()?
                .with_root_certificates(cert_store)
                .with_no_client_auth(),
        );

        let test_server = TestServer::start_with_tls(EMPTY_RESPONSE.into())?;
        let addr = test_server.addr();
        let url = Url::parse(&format!(
            "https://{}:{}/path?query=foo#fragment",
            addr.ip().to_string(),
            addr.port()
        ))
        .unwrap();
        let response = HttpClient::new(tls_client_config).get(&url)?.no_body()?;

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
}
