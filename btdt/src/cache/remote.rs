mod http;

use crate::cache::remote::http::{HttpClientError, HttpRequest, HttpResponse, ReadResponseBody};
use crate::cache::{Cache, CacheHit};
use crate::error::{IoPathError, IoPathResult, WithPath};
use crate::util::close::Close;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io;
use std::io::{BufRead, BufReader, BufWriter, ErrorKind, Read, Write};
use url::Url;

pub struct RemoteCache {
    base_url: Url,
    cache_id: String,
}

impl RemoteCache {
    pub fn new(base_url: &Url, cache_id: &str) -> Result<Self, RemoteCacheError> {
        Ok(RemoteCache {
            base_url: base_url
                .join("api/caches/")
                .expect("failed to join API path")
                .join(cache_id)
                .map_err(|err| RemoteCacheError::InvalidCacheId(cache_id.to_string(), err))?,
            cache_id: cache_id.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RemoteCacheError {
    InvalidCacheId(String, url::ParseError),
    HttpError { status: u16 },
}

impl Display for RemoteCacheError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCacheId(cache_id, _) => {
                f.write_str("invalid cache ID: ")?;
                f.write_str(cache_id)
            }
            Self::HttpError { status } => f.write_fmt(format_args!("http error: {status}")),
        }
    }
}

impl Error for RemoteCacheError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidCacheId(_, err) => Some(err),
            Self::HttpError { .. } => None,
        }
    }
}

pub struct RemoteWriter;

impl Write for RemoteWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        todo!()
    }

    fn flush(&mut self) -> io::Result<()> {
        todo!()
    }
}

impl Close for RemoteWriter {
    fn close(self) -> io::Result<()> {
        todo!()
    }
}

impl Cache for RemoteCache {
    type Reader = HttpResponse<ReadResponseBody>;
    type Writer = RemoteWriter;

    fn get<'a>(&self, keys: &[&'a str]) -> IoPathResult<Option<CacheHit<'a, Self::Reader>>> {
        if keys.len() < 1 {
            return Ok(None);
        }
        let mut url = self.base_url.clone();
        for key in keys {
            url.query_pairs_mut().append_pair("key", key);
        }
        let try_request = || HttpRequest::get(&url)?.no_body()?.read_status();
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
            size_hint: size_hint.unwrap_or(0),
            reader,
        }))
    }

    fn set(&self, keys: &[&str]) -> IoPathResult<Self::Writer> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::http::tests::{EMPTY_RESPONSE, TestServer};
    use super::*;
    use std::io;

    #[test]
    fn test_get_returns_none_for_empty_keys() {
        let test_server = TestServer::start(EMPTY_RESPONSE.into()).unwrap();
        let cache = RemoteCache::new(test_server.base_url(), "cache-id").unwrap();
        assert!(cache.get(&[]).unwrap().is_none());
    }

    #[test]
    fn test_get_returns_non_for_cache_miss() -> io::Result<()> {
        let test_server = TestServer::start(EMPTY_RESPONSE.into()).unwrap();
        let addr = test_server.addr();
        let cache = RemoteCache::new(test_server.base_url(), "cache-id").unwrap();
        assert!(cache.get(&["non-existent"])?.is_none());

        assert_eq!(
            test_server.request()?,
            format!(
                "\
                GET /api/caches/cache-id?key=non-existent HTTP/1.1\r\n\
                Host: {}\r\n\
                Connection: close\r\n\
                User-Agent: btdt/{}\r\n\r\n\
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
        let cache = RemoteCache::new(test_server.base_url(), "cache-id").unwrap();
        let CacheHit {
            key,
            size_hint,
            mut reader,
        } = cache.get(&["non-existent", "existent"])?.unwrap();
        assert_eq!(key, "existent");
        assert_eq!(size_hint, 8);

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
                User-Agent: btdt/{}\r\n\r\n\
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
        let cache = RemoteCache::new(test_server.base_url(), "cache-id").unwrap();
        let error = cache.get(&["non-existent"]).err().unwrap().into_io_error();
        assert_eq!(
            error
                .into_inner()
                .unwrap()
                .downcast::<RemoteCacheError>()
                .unwrap(),
            Box::new(RemoteCacheError::HttpError { status: 404 }),
        );
        Ok(())
    }
}
