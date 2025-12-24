# Installation

There are multiple ways to install `btdt`. Choose the method from below that best fits your needs.

> [!NOTE]
> Currently, only Unix (Linux, macOS) systems are supported.

## Pre-compiled binaries

You can download pre-compiled binaries from the [GitHub Releases page](https://github.com/jgosmann/btdt/releases).
The archive contains a single executable binary `btdt`.
You might want to place it in your `$PATH` for easy access.

## Docker images

Docker images are available on [Docker Hub](https://hub.docker.com/r/jgosmann/btdt).
This allows to directly run `btdt` without installing it on your system:

```sh
docker run jgosmann/btdt btdt --help
```

However, you will have to mount the directories with the cache and the files to cache into the container.
This can be done with the [`--mount` or `--volume` option](https://docs.docker.com/engine/storage/volumes/#syntax).

The images use Semantic Versioning tags. For example, `jgosmann/btdt:0.1`
refers to the latest `v0.1.x` image.

## Build from source using Rust

If you have Rust installed, you can build `btdt` from source using `cargo`:

```sh
cargo install btdt
```
