use btdt::cache::CacheHit;
use poem::Body;
use poem_openapi::payload::Binary;
use poem_openapi::ApiResponse;
use std::io;
use std::io::Read;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

#[derive(ApiResponse)]
#[allow(clippy::enum_variant_names)]
pub enum GetFromCacheResponse {
    /// No matching key was found in the cache.
    #[oai(status = 204)]
    CacheMiss,
    /// The cache with the given ID does not exist.
    #[oai(status = 404)]
    CacheNotFound,
    /// The data was found in the cache and is returned as a binary response.
    #[oai(status = 200)]
    CacheHit(Binary<Body>),
}

impl<'a, R> From<CacheHit<'a, R>> for GetFromCacheResponse
where
    R: Read + Send + 'static,
{
    fn from(hit: CacheHit<R>) -> Self {
        GetFromCacheResponse::CacheHit(Binary(Body::from_async_read(AsyncReadAdapter(Box::new(
            hit.reader,
        )))))
    }
}

struct AsyncReadAdapter(Box<dyn Read + Send>);

impl AsyncRead for AsyncReadAdapter {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut inbuf = vec![0; buf.remaining()];
        let len = self.0.read(&mut inbuf)?;
        buf.put_slice(&inbuf[..len]);
        Poll::Ready(Ok(()))
    }
}
