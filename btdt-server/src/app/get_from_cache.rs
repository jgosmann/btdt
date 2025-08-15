use btdt::cache::CacheHit;
use poem::Body;
use poem_openapi::ApiResponse;
use poem_openapi::payload::Binary;
use std::cmp::min;
use std::io;
use std::io::Read;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};
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
        GetFromCacheResponse::CacheHit(Binary(Body::from_async_read(AsyncReadAdapter::new(
            Box::new(hit.reader),
        ))))
    }
}

struct AsyncReadAdapter {
    rx: Receiver<io::Result<Vec<u8>>>,
    buf: Vec<u8>,
    buf_pos: usize,
}

impl AsyncReadAdapter {
    fn new(mut reader: Box<dyn Read + Send>) -> Self {
        let (tx, rx) = mpsc::channel(10);
        spawn_blocking(move || {
            loop {
                let mut buf = vec![0; 1024];
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        buf.resize(n, 0u8);
                        if tx.blocking_send(Ok(buf)).is_err() {
                            break; // Channel closed
                        }
                    }
                    Err(e) => {
                        if tx.blocking_send(Err(e)).is_err() {
                            break; // Channel closed
                        }
                    }
                }
            }
        });
        Self {
            rx,
            buf: vec![],
            buf_pos: 0,
        }
    }
}

impl AsyncRead for AsyncReadAdapter {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.buf_pos >= self.buf.len() {
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(Ok(data))) => {
                    self.buf = data;
                    self.buf_pos = 0;
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                Poll::Ready(None) => return Poll::Ready(Ok(())), // Channel closed
                Poll::Pending => return Poll::Pending,           // No data available yet
            }
        }
        buf.put_slice(&self.buf[self.buf_pos..min(self.buf_pos + buf.remaining(), self.buf.len())]);
        self.buf_pos += buf.remaining();
        Poll::Ready(Ok(()))
    }
}
