//! Types and utilities for working with blob IDs.

use rand::rngs::StdRng;
use rand::{CryptoRng, RngCore, SeedableRng};

/// The size of a [BlobId] in bytes.
pub const BLOB_ID_SIZE: usize = 16;

/// A unique identifier for a blob in the cache.
pub type BlobId = [u8; BLOB_ID_SIZE];

/// Factory for generating new blob IDs from a random number generator.
#[derive(Debug)]
pub struct BlobIdFactory<R: CryptoRng + RngCore = StdRng> {
    rng: R,
}

impl<R: CryptoRng + RngCore> BlobIdFactory<R> {
    /// Creates a new blob ID factory with the given random number generator.
    pub fn new(rng: R) -> Self {
        Self { rng }
    }

    /// Generates a new blob ID.
    pub fn new_id(&mut self) -> BlobId {
        let mut bytes = [0; BLOB_ID_SIZE];
        self.rng.fill_bytes(&mut bytes);
        bytes
    }
}

impl Default for BlobIdFactory<StdRng> {
    fn default() -> BlobIdFactory<StdRng> {
        Self::new(StdRng::from_os_rng())
    }
}
