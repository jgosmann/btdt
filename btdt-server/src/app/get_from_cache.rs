use btdt::cache::CacheHit;
use bytes::BytesMut;
use futures_core::Stream;
use poem::Body;
use poem_openapi::ApiResponse;
use poem_openapi::payload::Binary;
use std::io;
use std::io::Read;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::task::spawn_blocking;

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
        GetFromCacheResponse::CacheHit(Binary(Body::from_bytes_stream(StreamAdapter::new(
            Box::new(hit.reader),
        ))))
    }
}

struct StreamAdapter {
    rx: Receiver<io::Result<bytes::Bytes>>,
}

impl StreamAdapter {
    fn new<R: Read + Send + 'static>(mut reader: R) -> Self {
        let (tx, rx) = mpsc::channel(10);
        spawn_blocking(move || {
            const MAX_BUF_SIZE: usize = 81_920;
            let mut buf = BytesMut::zeroed(MAX_BUF_SIZE);
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        if tx.blocking_send(Ok(buf.split_to(n).freeze())).is_err() {
                            break; // Channel closed
                        }
                    }
                    Err(e) => {
                        if tx.blocking_send(Err(e)).is_err() {
                            break; // Channel closed
                        }
                    }
                }

                if buf.capacity() < 1024 {
                    buf = BytesMut::zeroed(MAX_BUF_SIZE);
                }
            }
        });
        Self { rx }
    }
}

impl Stream for StreamAdapter {
    type Item = io::Result<bytes::Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}
