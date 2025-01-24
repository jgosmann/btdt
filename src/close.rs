use std::io;
use std::io::Write;
use std::ops::{Deref, DerefMut};

pub trait Close {
    fn close(self) -> io::Result<()>;
}

pub struct SelfClosing<T> {
    inner: T,
}

impl<T> SelfClosing<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> Deref for SelfClosing<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for SelfClosing<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> Close for SelfClosing<T> {
    fn close(self) -> io::Result<()> {
        Ok(())
    }
}

impl<T: Write> Write for SelfClosing<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
