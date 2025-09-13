use rand::RngCore;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::Path;

pub trait CreateFilled {
    fn create_filled(path: &Path, size: usize, rng: &mut impl RngCore) -> io::Result<File>;
}

impl CreateFilled for File {
    fn create_filled(path: &Path, size: usize, rng: &mut impl RngCore) -> io::Result<File> {
        let mut file = File::create(path)?;
        const MAX_BUF_SIZE: usize = 10_485_760; // 10 MiB
        let mut buf = vec![0; usize::min(size, MAX_BUF_SIZE)];
        let mut remaining = size;
        while remaining > 0 {
            let slice = &mut buf[..usize::min(remaining, MAX_BUF_SIZE)];
            rng.fill_bytes(slice);
            file.write_all(slice)?;
            remaining -= slice.len();
        }
        Ok(file)
    }
}
