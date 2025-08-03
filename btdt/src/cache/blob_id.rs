//! Types and utilities for working with blob IDs.

use rand::{CryptoRng, RngCore};
use std::sync::RwLock;

/// The size of a [BlobId] in bytes.
pub const BLOB_ID_SIZE: usize = 16;

/// A unique identifier for a blob in the cache.
pub type BlobId = [u8; BLOB_ID_SIZE];

/// Trait for providing a random bytes.
pub trait RngBytes {
    /// Returns a new random number generator.
    fn fill_bytes(&self, buf: &mut [u8]);
}

pub struct SharedRng<R: CryptoRng + RngCore>(RwLock<R>);

impl<R: CryptoRng + RngCore> SharedRng<R> {
    pub fn new(rng: R) -> Self {
        Self(RwLock::new(rng))
    }
}

impl<R: CryptoRng + RngCore> RngBytes for SharedRng<R> {
    fn fill_bytes(&self, buf: &mut [u8]) {
        self.0.write().unwrap().fill_bytes(buf)
    }
}

pub struct ThreadRng;

impl RngBytes for ThreadRng {
    fn fill_bytes(&self, buf: &mut [u8]) {
        rand::rngs::ThreadRng::default().fill_bytes(buf)
    }
}

/// Factory for generating new blob IDs from a random number generator.
#[derive(Debug)]
pub struct BlobIdFactory<R: RngBytes = ThreadRng> {
    rng: R,
}

impl<R: RngBytes> BlobIdFactory<R> {
    /// Creates a new blob ID factory with the given random number generator.
    pub fn new(rng: R) -> Self {
        Self { rng }
    }

    /// Generates a new blob ID.
    pub fn new_id(&self) -> BlobId {
        let mut bytes = [0; BLOB_ID_SIZE];
        self.rng.fill_bytes(&mut bytes);
        bytes
    }
}

impl Default for BlobIdFactory<ThreadRng> {
    fn default() -> BlobIdFactory<ThreadRng> {
        Self::new(ThreadRng)
    }
}
