use btdt::cache::local::LocalCache;
use btdt::cache::{Cache, CacheHit};
use btdt::storage::filesystem::FilesystemStorage;
use btdt::storage::in_memory::InMemoryStorage;
use btdt::util::close::Close;
use std::io;
use std::io::{Read, Write};

pub enum CacheDispatcher {
    InMemory(LocalCache<InMemoryStorage>),
    Filesystem(LocalCache<FilesystemStorage>),
}

impl Cache for CacheDispatcher {
    type Reader = Box<dyn Read + Send>;
    type Writer = CacheWriter;

    fn get<'a>(&self, keys: &[&'a str]) -> io::Result<Option<CacheHit<'a, Self::Reader>>> {
        Ok(match self {
            Self::InMemory(cache) => cache.get(keys)?.map(|CacheHit { key, reader }| CacheHit {
                key,
                reader: Box::new(reader) as Box<dyn Read + Send>,
            }),
            Self::Filesystem(cache) => cache.get(keys)?.map(|CacheHit { key, reader }| CacheHit {
                key,
                reader: Box::new(reader) as Box<dyn Read + Send>,
            }),
        })
    }

    fn set(&self, keys: &[&str]) -> io::Result<Self::Writer> {
        match self {
            Self::InMemory(cache) => cache.set(keys).map(CacheWriter::InMemory),
            Self::Filesystem(cache) => cache.set(keys).map(CacheWriter::Filesystem),
        }
    }
}

pub enum CacheWriter {
    InMemory(<LocalCache<InMemoryStorage> as Cache>::Writer),
    Filesystem(<LocalCache<FilesystemStorage> as Cache>::Writer),
}

impl Write for CacheWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::InMemory(writer) => writer.write(buf),
            Self::Filesystem(writer) => writer.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::InMemory(writer) => writer.flush(),
            Self::Filesystem(writer) => writer.flush(),
        }
    }
}

impl Close for CacheWriter {
    fn close(self) -> io::Result<()> {
        match self {
            Self::InMemory(writer) => writer.close(),
            Self::Filesystem(writer) => writer.close(),
        }
    }
}
