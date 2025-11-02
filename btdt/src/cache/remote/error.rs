use rustls::pki_types::InvalidDnsNameError;
use std::fmt::{Display, Formatter};
use std::io;

#[derive(Debug)]
pub enum HttpClientError {
    InvalidScheme(String),
    InvalidDnsName(String),
    MissingHost,
    UnsupportedFeature(&'static str),
    IoError(io::Error),
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
            Self::InvalidScheme(scheme) => write!(f, "unknown URL scheme: {}", scheme),
            Self::InvalidDnsName(hostname) => write!(f, "invalid DNS name: {}", hostname),
            Self::MissingHost => write!(f, "missing host"),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {}", feature),
            Self::IoError(err) => write!(f, "I/O error: {}", err),
            Self::TlsError(err) => write!(f, "TLS error: {}", err),
        }
    }
}

impl std::error::Error for HttpClientError {}

impl From<io::Error> for HttpClientError {
    fn from(err: io::Error) -> Self {
        HttpClientError::IoError(err)
    }
}

impl Into<io::Error> for HttpClientError {
    fn into(self) -> io::Error {
        match self {
            HttpClientError::InvalidScheme(_) => io::Error::new(io::ErrorKind::InvalidInput, self),
            HttpClientError::MissingHost => io::Error::new(io::ErrorKind::InvalidInput, self),
            HttpClientError::UnsupportedFeature(_) => {
                io::Error::new(io::ErrorKind::Unsupported, self)
            }
            HttpClientError::IoError(err) => err,
            HttpClientError::InvalidDnsName(_) => io::Error::new(io::ErrorKind::InvalidInput, self),
            HttpClientError::TlsError(_) => io::Error::new(io::ErrorKind::Other, self),
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
