# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.1](https://github.com/jgosmann/btdt/compare/btdt-cli-v0.4.0...btdt-cli-v0.4.1) - 2025-12-20

### Fixed

- Support trailing newlines for auth token

## [0.4.0](https://github.com/jgosmann/btdt/compare/btdt-cli-v0.3.5...btdt-cli-v0.4.0) - 2025-12-07

### Fixed

- [**breaking**] Disambiguate exit codes

### Other

- [**breaking**] Use actual API url for remote cache specification
- Fix clippy lint
- Use module/mod.rs structure consistently
- Update btdt API documentation
- clippy lint

## [0.3.5](https://github.com/jgosmann/btdt/compare/btdt-server-v0.3.4...btdt-server-v0.3.5) - 2025-12-04

### Other

- Allow to start health-check without `--root-cert`
- Allow to set trusted root cert for health check via environment variable

## [0.3.4](https://github.com/jgosmann/btdt/compare/btdt-cli-v0.3.3...btdt-cli-v0.3.4) - 2025-12-02

### Other

- update Cargo.lock dependencies

## [0.3.3](https://github.com/jgosmann/btdt/compare/btdt-cli-v0.3.2...btdt-cli-v0.3.3) - 2025-11-30

### Added

- Add `--root-cert` flag to specify custom TLS root certificates
- Implement background cleanup job in btdt-server
- Implement btdt-server health-check command

### Other

- Deny unsafe code without safety comment

## [0.3.2](https://github.com/jgosmann/btdt/compare/btdt-cli-v0.3.1...btdt-cli-v0.3.2) - 2025-11-18

### Other

- Bump version

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
