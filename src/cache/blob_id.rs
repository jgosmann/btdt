use rand::rngs::StdRng;
use rand::{CryptoRng, RngCore, SeedableRng};

pub const BLOB_ID_SIZE: usize = 16;
pub type BlobId = [u8; BLOB_ID_SIZE];

#[derive(Debug)]
pub struct BlobIdFactory<R: CryptoRng + RngCore = StdRng> {
    rng: R,
}

impl<R: CryptoRng + RngCore> BlobIdFactory<R> {
    pub fn new(rng: R) -> Self {
        Self { rng }
    }

    pub fn new_id(&mut self) -> BlobId {
        let mut bytes = [0; BLOB_ID_SIZE];
        self.rng.fill_bytes(&mut bytes);
        bytes
    }
}

impl Default for BlobIdFactory<StdRng> {
    fn default() -> BlobIdFactory<StdRng> {
        Self::new(StdRng::from_entropy())
    }
}
