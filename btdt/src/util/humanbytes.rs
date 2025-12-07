//! A utility module for parsing human-readable byte size strings into their corresponding
//! byte values.

use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::num::ParseIntError;

/// An error that can occur while parsing a human-readable byte size string.
#[derive(Debug)]
pub enum ParserError {
    /// An error that occurred while parsing an integer value.
    ParseIntError(ParseIntError),
    /// An invalid unit prefix was encountered.
    InvalidUnitPrefix(String),
    /// The input string consists only of whitespace.
    OnlyWhitespace,
}

impl Display for ParserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ParserError::ParseIntError(e) => write!(f, "Failed to parse integer: {e}"),
            ParserError::InvalidUnitPrefix(prefix) => {
                write!(f, "Invalid unit prefix: '{prefix}'")
            }
            ParserError::OnlyWhitespace => write!(f, "Input consists only of whitespace"),
        }
    }
}

impl Error for ParserError {}

impl From<ParseIntError> for ParserError {
    fn from(e: ParseIntError) -> Self {
        ParserError::ParseIntError(e)
    }
}

/// Parses a human-readable byte size string into its corresponding byte value.
pub fn parse_bytes_from_str(input: &str) -> Result<u64, ParserError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ParserError::OnlyWhitespace);
    }

    let mut pos = 0;
    let mut accumulator = 0;
    while pos < input.len() {
        pos += next_token(&input[pos..], u8::is_ascii_whitespace).len();
        if pos >= input.len() {
            break;
        }
        let value_token = next_token(&input[pos..], u8::is_ascii_digit);
        pos += value_token.len();
        pos += next_token(&input[pos..], u8::is_ascii_whitespace).len();
        let unit_prefix_token = next_token(&input[pos..], u8::is_ascii_alphabetic);
        pos += unit_prefix_token.len();
        let value_factor: u64 = value_token.parse()?;
        let unit_prefix_factor = parse_unit_prefix_from_str(unit_prefix_token)?;
        accumulator += value_factor * unit_prefix_factor;
    }
    Ok(accumulator)
}

fn next_token(input: &str, predicate: impl FnMut(&u8) -> bool) -> &str {
    &input[..input
        .as_bytes()
        .iter()
        .copied()
        .take_while(predicate)
        .count()]
}

fn parse_unit_prefix_from_str(prefix: &str) -> Result<u64, ParserError> {
    match prefix.strip_suffix("B").unwrap_or(prefix) {
        "" => Ok(1),
        "k" | "K" => Ok(1_000),
        "M" => Ok(1_000_000),
        "G" => Ok(1_000_000_000),
        "T" => Ok(1_000_000_000_000),
        "P" => Ok(1_000_000_000_000_000),
        "E" => Ok(1_000_000_000_000_000_000),
        "Ki" => Ok(1 << 10),
        "Mi" => Ok(1 << 20),
        "Gi" => Ok(1 << 30),
        "Ti" => Ok(1 << 40),
        "Pi" => Ok(1 << 50),
        "Ei" => Ok(1 << 60),
        _ => Err(ParserError::InvalidUnitPrefix(prefix.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bytes_from_str_without_unit_prefix() {
        assert_eq!(parse_bytes_from_str("0").unwrap(), 0);
        assert_eq!(parse_bytes_from_str("123").unwrap(), 123);
        assert_eq!(parse_bytes_from_str("27462").unwrap(), 27462);
    }

    #[test]
    fn test_parse_bytes_from_str_with_metric_unit_prefix() {
        assert_eq!(parse_bytes_from_str("1k").unwrap(), 1_000);
        assert_eq!(parse_bytes_from_str("1K").unwrap(), 1_000);
        assert_eq!(parse_bytes_from_str("1M").unwrap(), 1_000_000);
        assert_eq!(parse_bytes_from_str("1G").unwrap(), 1_000_000_000);
        assert_eq!(parse_bytes_from_str("1T").unwrap(), 1_000_000_000_000);
        assert_eq!(parse_bytes_from_str("1P").unwrap(), 1_000_000_000_000_000);
        assert_eq!(
            parse_bytes_from_str("1E").unwrap(),
            1_000_000_000_000_000_000
        );
        assert_eq!(parse_bytes_from_str("1kB").unwrap(), 1_000);
        assert_eq!(parse_bytes_from_str("1KB").unwrap(), 1_000);
        assert_eq!(parse_bytes_from_str("1MB").unwrap(), 1_000_000);
        assert_eq!(parse_bytes_from_str("1GB").unwrap(), 1_000_000_000);
        assert_eq!(parse_bytes_from_str("1TB").unwrap(), 1_000_000_000_000);
        assert_eq!(parse_bytes_from_str("1PB").unwrap(), 1_000_000_000_000_000);
        assert_eq!(
            parse_bytes_from_str("1EB").unwrap(),
            1_000_000_000_000_000_000
        );
    }

    #[test]
    fn test_parse_bytes_from_str_with_binary_unit_prefix() {
        assert_eq!(parse_bytes_from_str("1Ki").unwrap(), 1 << 10);
        assert_eq!(parse_bytes_from_str("1Mi").unwrap(), 1 << 20);
        assert_eq!(parse_bytes_from_str("1Gi").unwrap(), 1 << 30);
        assert_eq!(parse_bytes_from_str("1Ti").unwrap(), 1 << 40);
        assert_eq!(parse_bytes_from_str("1Pi").unwrap(), 1 << 50);
        assert_eq!(parse_bytes_from_str("1Ei").unwrap(), 1 << 60);
        assert_eq!(parse_bytes_from_str("1KiB").unwrap(), 1 << 10);
        assert_eq!(parse_bytes_from_str("1MiB").unwrap(), 1 << 20);
        assert_eq!(parse_bytes_from_str("1GiB").unwrap(), 1 << 30);
        assert_eq!(parse_bytes_from_str("1TiB").unwrap(), 1 << 40);
        assert_eq!(parse_bytes_from_str("1PiB").unwrap(), 1 << 50);
        assert_eq!(parse_bytes_from_str("1EiB").unwrap(), 1 << 60);
    }

    #[test]
    fn test_parse_bytes_from_str_ignore_whitespace_between_and_around_components() {
        assert_eq!(parse_bytes_from_str(" \t 1  k \n ").unwrap(), 1_000);
    }

    #[test]
    fn test_parse_bytes_from_str_supports_multiple_components() {
        assert_eq!(
            parse_bytes_from_str("1Gi 200Mi").unwrap(),
            1024 * 1024 * 1024 + 200 * 1024 * 1024
        );
        assert_eq!(
            parse_bytes_from_str("1K 200Mi").unwrap(),
            1000 + 200 * 1024 * 1024
        );
        assert_eq!(parse_bytes_from_str("1K 42").unwrap(), 1000 + 42);
        assert_eq!(parse_bytes_from_str("10 20k").unwrap(), 10 + 20 * 1000);
    }
}
