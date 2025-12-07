//! Library module for btdt-server
//!
//! This library module exists for two reasons:
//!
//! 1. To share code between the `btdt-server` binary crate and benchmarks.
//! 2. To share code with the integration tests of the `btdt-server` and `btdt-cli`.

pub mod asyncio;
#[cfg(feature = "test")]
pub mod test_server;
