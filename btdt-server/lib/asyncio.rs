//! Helpers for dealing with asynchronous I/O that should be available for benchmarks.

use bytes::BytesMut;
use futures_core::Stream;
use std::io;
use std::io::Read;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::task::spawn_blocking;

/// An adapter to convert a blocking `Read` into an async `Stream` of `Bytes`.
pub struct StreamAdapter {
    rx: Receiver<io::Result<bytes::Bytes>>,
}

impl StreamAdapter {
    /// Creates a new `StreamAdapter` from a blocking `Read` implementor.
    pub fn new<R: Read + Send + 'static>(mut reader: R, size_hint: Option<u64>) -> Self {
        let (tx, rx) = mpsc::channel(10);
        spawn_blocking(move || {
            const MAX_BUF_SIZE: usize = 512 * 1024;
            const REALLOCATION_THRESHOLD: usize = 1024;
            let buf_size = usize::min(
                size_hint
                    .map(|hint| hint as usize + REALLOCATION_THRESHOLD)
                    .unwrap_or(MAX_BUF_SIZE),
                MAX_BUF_SIZE,
            );
            let mut buf = BytesMut::zeroed(buf_size);
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

                if buf.capacity() < REALLOCATION_THRESHOLD {
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
