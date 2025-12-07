//! Provides a remote cache implementation using HTTP.

use crate::cache::remote::RemoteCacheError::MissingCacheId;
use crate::cache::{Cache, CacheHit};
use crate::error::{IoPathError, IoPathResult, WithPath};
use crate::util::close::Close;
pub use crate::util::http;
use crate::util::http::error::HttpClientError;
use crate::util::http::{
    AwaitingRequestBody, AwaitingRequestHeaders, ChunkedTransferEncoding, HttpClient, HttpRequest,
    HttpResponse, OptionTransferEncoding, ReadResponseBody,
};
use biscuit_auth::UnverifiedBiscuit;
use biscuit_auth::macros::block;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io;
use std::io::{ErrorKind, Write};
use std::time::{Duration, SystemTime};
use url::Url;

/// A remote cache that stores data via the btdt HTTP API.
pub struct RemoteCache {
    base_url: Url,
    cache_id: String,
    client: HttpClient,
    token: UnverifiedBiscuit,
}

impl RemoteCache {
    /// Creates a new remote cache with the given base URL, HTTP client, and authentication token.
    pub fn new(
        base_url: Url,
        client: HttpClient,
        token: UnverifiedBiscuit,
    ) -> Result<Self, RemoteCacheError> {
        let cache_id = base_url
            .path_segments()
            .ok_or(MissingCacheId)?
            .next_back()
            .ok_or(MissingCacheId)?
            .to_string();
        Ok(RemoteCache {
            base_url,
            cache_id,
            client,
            token,
        })
    }
}

/// An error that can occur when using the remote cache.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RemoteCacheError {
    /// The provided URL does not contain a cache ID.
    MissingCacheId,
    /// An HTTP error occurred.
    HttpError {
        /// The HTTP status code.
        status: u16,
    },
}

impl Display for RemoteCacheError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCacheId => write!(f, "missing cache ID in URL"),
            Self::HttpError { status } => write!(f, "http error: {status}"),
        }
    }
}

impl Error for RemoteCacheError {}

/// A cache writer writing ot the remote cache.
pub struct RemoteWriter(HttpRequest<AwaitingRequestBody<ChunkedTransferEncoding>>);

impl Write for RemoteWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Close for RemoteWriter {
    fn close(self) -> io::Result<()> {
        let (status, _) = self
            .0
            .response()
            .map_err(Into::<io::Error>::into)?
            .read_status()
            .map_err(Into::<io::Error>::into)?;

        if !status.is_success() {
            return Err(io::Error::other(RemoteCacheError::HttpError {
                status: status.code_u16(),
            }));
        }

        Ok(())
    }
}

impl Cache for RemoteCache {
    type Reader = HttpResponse<ReadResponseBody>;
    type Writer = RemoteWriter;

    fn get<'a>(&self, keys: &[&'a str]) -> IoPathResult<Option<CacheHit<'a, Self::Reader>>> {
        if keys.is_empty() {
            return Ok(None);
        }
        let mut url = self.base_url.clone();
        for key in keys {
            url.query_pairs_mut().append_pair("key", key);
        }
        let try_request = || {
            let mut request = self.client.get(&url)?;
            self.add_auth_header(&mut request, Operation::Get, &self.cache_id)?;
            request.no_body()?.read_status()
        };
        let (status, mut response) = try_request()
            .map_err(HttpClientError::into)
            .with_path(url.as_str())?;

        if !status.is_success() {
            return Err(IoPathError::new_no_path(io::Error::other(
                RemoteCacheError::HttpError {
                    status: status.code_u16(),
                },
            )));
        }

        if status.code() == "204" {
            return Ok(None);
        }

        let mut size_hint = None;
        let mut hit_key = None;
        while let Some(header) = response
            .read_next_header()
            .map_err(HttpClientError::into)
            .with_path(url.as_str())?
        {
            if size_hint.is_none() && header.key().eq_ignore_ascii_case("content-length") {
                size_hint = header.value().parse::<u64>().ok();
            }
            if hit_key.is_none() && header.key().eq_ignore_ascii_case("btdt-cache-key") {
                hit_key = keys.iter().find(|&&key| key == header.value());
            }
            if size_hint.is_some() && hit_key.is_some() {
                break;
            }
        }
        let reader = response
            .read_body()
            .map_err(HttpClientError::into)
            .with_path(url.as_str())?;

        Ok(Some(CacheHit {
            key: hit_key
                .ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidData,
                        "missing Btdt-Cache-Key header in response",
                    )
                })
                .with_path(url.as_str())?,
            size_hint,
            reader,
        }))
    }

    fn set(&self, keys: &[&str]) -> IoPathResult<Self::Writer> {
        let mut url = self.base_url.clone();
        for key in keys {
            url.query_pairs_mut().append_pair("key", key);
        }

        let try_request = || {
            let mut request = self.client.put(&url)?;
            self.add_auth_header(&mut request, Operation::Put, &self.cache_id)?;
            request.body()
        };
        let request = try_request()
            .map_err(HttpClientError::into)
            .with_path(url.as_str())?;
        Ok(RemoteWriter(request))
    }
}

enum Operation {
    Get,
    Put,
}

impl AsRef<str> for Operation {
    fn as_ref(&self) -> &str {
        match self {
            Operation::Get => "get",
            Operation::Put => "put",
        }
    }
}

impl RemoteCache {
    fn add_auth_header<T: OptionTransferEncoding>(
        &self,
        request: &mut HttpRequest<AwaitingRequestHeaders<T>>,
        operation: Operation,
        cache_id: &str,
    ) -> http::Result<()> {
        let expiration = SystemTime::now()
            .checked_add(Duration::from_secs(5 * 60))
            .expect("time overflow");
        request.header(
            "Authorization",
            &format!(
                "Bearer {}",
                self.token
                    .append(block!(
                        "\
                            check if operation({operation});\
                            check if cache({cache});\
                            check if time($time), $time < {expiration};\
                        ",
                        operation = operation.as_ref(),
                        cache = cache_id,
                        expiration = expiration,
                    ))
                    .unwrap()
                    .to_base64()
                    .unwrap()
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::http::tests::{EMPTY_RESPONSE, TestServer};
    use super::*;
    use biscuit_auth::KeyPair;
    use biscuit_auth::macros::biscuit;
    use std::io;
    use std::io::Read;

    fn auth_token() -> UnverifiedBiscuit {
        UnverifiedBiscuit::from(
            &biscuit!("")
                .build(&KeyPair::new())
                .unwrap()
                .to_vec()
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn test_get_returns_none_for_empty_keys() {
        let test_server = TestServer::start(EMPTY_RESPONSE.into()).unwrap();
        let cache = RemoteCache::new(
            test_server.base_url().join("api/caches/cache-id").unwrap(),
            HttpClient::default().unwrap(),
            auth_token(),
        )
        .unwrap();
        assert!(cache.get(&[]).unwrap().is_none());
    }

    #[test]
    fn test_get_returns_non_for_cache_miss() -> io::Result<()> {
        let test_server = TestServer::start(EMPTY_RESPONSE.into()).unwrap();
        let addr = test_server.addr();
        let cache = RemoteCache::new(
            test_server.base_url().join("api/caches/cache-id").unwrap(),
            HttpClient::default().unwrap(),
            auth_token(),
        )
        .unwrap();
        assert!(cache.get(&["non-existent"])?.is_none());

        assert_eq!(
            test_server.request()?,
            format!(
                "\
                GET /api/caches/cache-id?key=non-existent HTTP/1.1\r\n\
                Host: {}\r\n\
                Connection: close\r\n\
                User-Agent: btdt/{}\r\n\
                Authorization: <auth-header-value>\r\n\r\n\
            ",
                addr.ip(),
                env!("CARGO_PKG_VERSION")
            )
        );

        Ok(())
    }

    #[test]
    fn test_get_returns_data_for_cache_hit() -> io::Result<()> {
        let test_server = TestServer::start(
            "HTTP/1.1 200 Ok\r\nBtdt-Cache-Key: existent\r\nContent-Length: 8\r\n\r\nHello!\r\n"
                .into(),
        )
        .unwrap();
        let addr = test_server.addr();
        let cache = RemoteCache::new(
            test_server.base_url().join("api/caches/cache-id").unwrap(),
            HttpClient::default().unwrap(),
            auth_token(),
        )
        .unwrap();
        let CacheHit {
            key,
            size_hint,
            mut reader,
        } = cache.get(&["non-existent", "existent"])?.unwrap();
        assert_eq!(key, "existent");
        assert_eq!(size_hint, Some(8));

        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        assert_eq!(buf, "Hello!\r\n");

        assert_eq!(
            test_server.request()?,
            format!(
                "\
                GET /api/caches/cache-id?key=non-existent&key=existent HTTP/1.1\r\n\
                Host: {}\r\n\
                Connection: close\r\n\
                User-Agent: btdt/{}\r\n\
                Authorization: <auth-header-value>\r\n\r\n\
            ",
                addr.ip(),
                env!("CARGO_PKG_VERSION")
            )
        );

        Ok(())
    }

    #[test]
    fn test_get_returns_error_for_non_success_http_status() -> io::Result<()> {
        let test_server =
            TestServer::start("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".into())
                .unwrap();
        let cache = RemoteCache::new(
            test_server.base_url().join("api/caches/cache-id").unwrap(),
            HttpClient::default().unwrap(),
            auth_token(),
        )
        .unwrap();
        let error = cache.get(&["non-existent"]).err().unwrap().into_io_error();
        match *error
            .into_inner()
            .unwrap()
            .downcast::<RemoteCacheError>()
            .unwrap()
        {
            RemoteCacheError::HttpError { status } => {
                assert_eq!(status, 404);
            }
            _ => panic!("unexpected error type"),
        }
        Ok(())
    }

    #[test]
    fn test_set_sends_data_to_remote_cache() -> io::Result<()> {
        let test_server = TestServer::start(EMPTY_RESPONSE.into()).unwrap();
        let addr = test_server.addr();
        let cache = RemoteCache::new(
            test_server.base_url().join("api/caches/cache-id").unwrap(),
            HttpClient::default().unwrap(),
            auth_token(),
        )
        .unwrap();
        let mut writer = cache.set(&["key1", "key2"])?;

        writer.write_all(b"Test data")?;
        writer.close()?;

        assert_eq!(
            test_server.request()?,
            format!(
                "\
                PUT /api/caches/cache-id?key=key1&key=key2 HTTP/1.1\r\n\
                Host: {}\r\n\
                Connection: close\r\n\
                User-Agent: btdt/{}\r\n\
                Authorization: <auth-header-value>\r\n\
                Transfer-Encoding: chunked\r\n\
                \r\n\
                Test data",
                addr.ip(),
                env!("CARGO_PKG_VERSION")
            )
        );

        Ok(())
    }
}
