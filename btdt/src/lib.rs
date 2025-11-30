//! `btdt` is a tool for flexible caching files in CI pipelines.
//!
//! You are reading the library API documentation. If you are not looking to integrate `btdt` into
//! your own Rust project, but to use it your CI pipelines, you probably want to read the
//! [user guide](https://jgosmann.github.io/btdt/).
//!
//! `btdt` makes use of three main concepts:
//!
//! - **Storage**: A [storage] is a place where files are stored, for example the local filesystem.
//! - **Cache**: A [cache] manages keys and associated data, and might use a storage to store that
//!   data. It can also take care of cleaning old entry based on age or cache size.
//! - **Pipeline**: A [pipeline] defines how multiple files a processed to be stored in the cache,
//!   e.g. by archiving them in TAR format and potentially compressing them.
//!
//! This makes the [pipeline] module the high-level interface to the `btdt` library.

pub mod cache;
pub mod error;
pub mod pipeline;
pub mod storage;

pub mod util {
    //! Collects traits, functions, etc. that are not directly related to the main concepts of `btdt`.

    pub(crate) mod clock;
    pub mod close;
    pub(crate) mod encoding;
    pub mod http;
    pub mod humanbytes;
}
pub mod test_util {
    //! Utilities for testing `btdt` code.
    //!
    //! These are not intended to be used in production code.

    pub mod fs;
    pub mod fs_spec;
}
