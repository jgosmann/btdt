# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1](https://github.com/jgosmann/btdt/compare/btdt-cli-v0.3.0...btdt-cli-v0.3.1) - 2025-11-18

### Other

- Bump versions

## [0.3.0](https://github.com/jgosmann/btdt/compare/btdt-cli-v0.2.0...btdt-cli-v0.3.0) - 2025-11-16

### Other

- Address clippy warning about enum size difference
- Fix dead code warnings
- Implement authorization
- Support remote cache in CLI
- Replace enum dispatch with `Box<dyn ...>`
- Allow to set TLS configuration from outside RemoteCache
- Address compiler warnings and clippy lints
- Implement TLS support
- Remove unused dependency
- Implement integration test for remote cache and cache server
- Implement setting cache entries with remote cache
- Implement sending with chunked transfer encoding in HTTP client
- Make size_hint optional
- Implement retrieval from remote cache
- Implement simple HTTP/1.1 client

## 0.1.0 - 2025-03-01

Initial release.
