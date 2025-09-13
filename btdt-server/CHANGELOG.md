# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/jgosmann/btdt/releases/tag/btdt-server-v0.1.0) - 2025-09-13

### Other

- Make use of `size_hint` in `StreamAdapter`
- Provide file size when getting a file from cache
- Optimize StreamAdapter buffer size
- Benchmark StreamAdapter
- Move StreamAdapter to separate module
- Improve reading of cache entry
- Use spawn_blocking to offload blocking I/O when reading from cache
- Replace put implementation with SyncIoBridge and spawn_blocking
- Set correct API prefix
- Upgrade to Rust edition 2024
- Implement remote cache endpoints
- Print version info on server startup
- Implement TLS support
- Test config for enabling/disabling API docs
- Test parsing of config
- Allow disabling API docs
- Add configuration for bind addresses to btdt-server
- Add Swagger UI documentation
- Add integration test for btdt-server
- Add btdt-server with simple health endpoint
