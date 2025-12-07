//! Error types for HTTP client operations.

use rustls::pki_types::InvalidDnsNameError;
use std::fmt::{Display, Formatter};
use std::io;

/// An error that can occur during HTTP client operations.
#[derive(Debug)]
pub enum HttpClientError {
    /// An invalid URL scheme was encountered.
    InvalidScheme(String),
    /// An invalid DNS name was encountered.
    InvalidDnsName(String),
    /// The URL is missing a host.
    MissingHost,
    /// An unsupported feature was requested.
    UnsupportedFeature(&'static str),
    /// An I/O error occurred.
    IoError(io::Error),
    /// A TLS error occurred.
    TlsError(rustls::Error),
}

impl HttpClientError {
    pub fn invalid_data(msg: &str) -> Self {
        HttpClientError::IoError(io::Error::new(io::ErrorKind::InvalidData, msg))
    }
}

impl Display for HttpClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScheme(scheme) => write!(f, "unknown URL scheme: {scheme}"),
            Self::InvalidDnsName(hostname) => write!(f, "invalid DNS name: {hostname}"),
            Self::MissingHost => write!(f, "missing host"),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {feature}"),
            Self::IoError(err) => write!(f, "I/O error: {err}"),
            Self::TlsError(err) => write!(f, "TLS error: {err}"),
        }
    }
}

impl std::error::Error for HttpClientError {}

impl From<io::Error> for HttpClientError {
    fn from(err: io::Error) -> Self {
        HttpClientError::IoError(err)
    }
}

impl From<HttpClientError> for io::Error {
    fn from(value: HttpClientError) -> Self {
        match value {
            HttpClientError::InvalidScheme(_) => io::Error::new(io::ErrorKind::InvalidInput, value),
            HttpClientError::MissingHost => io::Error::new(io::ErrorKind::InvalidInput, value),
            HttpClientError::UnsupportedFeature(_) => {
                io::Error::new(io::ErrorKind::Unsupported, value)
            }
            HttpClientError::IoError(err) => err,
            HttpClientError::InvalidDnsName(_) => {
                io::Error::new(io::ErrorKind::InvalidInput, value)
            }
            HttpClientError::TlsError(_) => io::Error::other(value),
        }
    }
}

impl From<InvalidDnsNameError> for HttpClientError {
    fn from(value: InvalidDnsNameError) -> Self {
        Self::InvalidDnsName(value.to_string())
    }
}

impl From<rustls::Error> for HttpClientError {
    fn from(value: rustls::Error) -> Self {
        Self::TlsError(value)
    }
}
