use crate::util::close::Close;
use std::io;
use std::io::{Read, Write};

pub mod blob_id;
pub mod local;
mod meta;

pub trait Cache {
    type Reader: Read;
    type Writer: Write + Close;

    fn get(&self, keys: &[&str]) -> io::Result<Option<Self::Reader>>;
    fn set(&mut self, keys: &[&str]) -> io::Result<Self::Writer>;
}
