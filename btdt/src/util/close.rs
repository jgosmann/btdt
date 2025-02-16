//! Provides the [Close] trait and a [SelfClosing] wrapper for types that need to be finalized.

use std::io;
use std::io::Write;
use std::ops::{Deref, DerefMut};

/// A trait for types that need to be finalized.
///
/// This trait is used to finalize operations where this finalization might fail. Using
/// [Close::close] instead of just dropping allows to retrieve potential errors and handle them.
///
/// For a type implementing [Close], it should be considered to also implement [Drop] and panic in
/// case of an error.
pub trait Close {
    fn close(self) -> io::Result<()>;
}

/// A wrapper type to provide a [Close] implementation that does nothing.
///
/// This can be used with types that do not need to be finalized, but are used in a context that
/// requires a [Close] implementation.
///
/// # Examples
///
/// ```rust
/// use btdt::util::close::{Close, SelfClosing};
/// SelfClosing::new(42).close().unwrap();
/// ```
pub struct SelfClosing<T> {
    inner: T,
}

impl<T> SelfClosing<T> {
    /// Creates a new [SelfClosing] wrapper around the given value.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Unwraps the inner value.
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
