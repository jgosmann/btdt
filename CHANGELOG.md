# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/jgosmann/btdt/compare/btdt-cli-v0.1.0...btdt-cli-v0.2.0) - 2025-09-13

### Other

- Update formatting
- Upgrade to Rust edition 2024
- Allow caches to be shared across threads
- Make use of improved format string syntax
- Extend motivation section with caching problems on Tekton
- Provide file size when getting a file from cache
- Provide file size when getting a file from storage
- Benchmark StreamAdapter
- Extract method for creating a filled file
- Remove BufReader
- Add benchmarks for store/restore
- Collapse if statements
- Fix new clippy warning
- Implement Send for cache Meta
- Make InMemoryStorage fully Send + Sync
- Remove no longer required RefCell from LocalCache
- Eliminate warning about unused variable
- Use thread-safe interior mutability for storage
- Remove unnecessary &mut from Storage::exists_file
- Simplify error constructor

## 0.1.0 - 2025-03-01

Initial release.
