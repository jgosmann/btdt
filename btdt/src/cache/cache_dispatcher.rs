use crate::cache::local::LocalCache;
use crate::cache::remote::RemoteCache;
use crate::cache::{Cache, CacheHit};
use crate::error::IoPathResult;
use crate::storage::filesystem::FilesystemStorage;
use crate::storage::in_memory::InMemoryStorage;
use crate::util::close::Close;
use std::io;
use std::io::{Read, Write};

pub enum CacheDispatcher {
    InMemory(LocalCache<InMemoryStorage>),
    Filesystem(LocalCache<FilesystemStorage>),
    Remote(RemoteCache),
}

impl Cache for CacheDispatcher {
    type Reader = Box<dyn Read + Send>;
    type Writer = CacheWriter;

    fn get<'a>(&self, keys: &[&'a str]) -> IoPathResult<Option<CacheHit<'a, Self::Reader>>> {
        Ok(match self {
            Self::InMemory(cache) => cache.get(keys)?.map(
                |CacheHit {
                     key,
                     reader,
                     size_hint,
                 }| CacheHit {
                    key,
                    reader: Box::new(reader) as Box<dyn Read + Send>,
                    size_hint,
                },
            ),
            Self::Filesystem(cache) => cache.get(keys)?.map(
                |CacheHit {
                     key,
                     reader,
                     size_hint,
                 }| CacheHit {
                    key,
                    reader: Box::new(reader) as Box<dyn Read + Send>,
                    size_hint,
                },
            ),
            CacheDispatcher::Remote(cache) => cache.get(keys)?.map(
                |CacheHit {
                     key,
                     reader,
                     size_hint,
                 }| CacheHit {
                    key,
                    reader: Box::new(reader) as Box<dyn Read + Send>,
                    size_hint,
                },
            ),
        })
    }

    fn set(&self, keys: &[&str]) -> IoPathResult<Self::Writer> {
        match self {
            Self::InMemory(cache) => cache.set(keys).map(CacheWriter::InMemory),
            Self::Filesystem(cache) => cache.set(keys).map(CacheWriter::Filesystem),
            CacheDispatcher::Remote(cache) => cache.set(keys).map(CacheWriter::Remote),
        }
    }
}

pub enum CacheWriter {
    InMemory(<LocalCache<InMemoryStorage> as Cache>::Writer),
    Filesystem(<LocalCache<FilesystemStorage> as Cache>::Writer),
    Remote(<RemoteCache as Cache>::Writer),
}

impl Write for CacheWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::InMemory(writer) => writer.write(buf),
            Self::Filesystem(writer) => writer.write(buf),
            CacheWriter::Remote(writer) => writer.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::InMemory(writer) => writer.flush(),
            Self::Filesystem(writer) => writer.flush(),
            CacheWriter::Remote(writer) => writer.flush(),
        }
    }
}

impl Close for CacheWriter {
    fn close(self) -> io::Result<()> {
        match self {
            Self::InMemory(writer) => writer.close(),
            Self::Filesystem(writer) => writer.close(),
            CacheWriter::Remote(writer) => writer.close(),
        }
    }
}
