# Deployment

The `btdt-server` is a server application that allows to store and retrieve cached files with `btdt` on this server.

## Installation

There are multiple ways to install `btdt-server`. Choose the method from below that best fits your needs.

> [!NOTE]
> Currently, only Unix (Linux, macOS) systems are supported.

### Pre-compiled binaries

You can download pre-compiled binaries from the [GitHub Releases
page](https://github.com/jgosmann/btdt/releases?q=btdt-server&expanded=true) (look for `btdt-server` releases).
The archive contains a single executable binary `btdt-server` that will start the server.

### Docker images

Docker images are available on [Docker Hub](https://hub.docker.com/r/jgosmann/btdt-server).

The images use Semantic Versioning tags. For example, `jgosmann/btdt-server:0.1` refers to the latest `v0.1.x` image.

When running the container, you likely want to mount a few files or directories into the container:

- The directory where caches are stored, so that they are persisted across container restarts.
- The configuration file (default: `/config.toml`).
- The file with the private key for authentication (default: `/auth_private_key.pem`).
- If using TLS, the PKCS#12 file with the TLS certificate and private key.

Note that, if you are using TLS, you will have to override the default health check command with:

```Dockerfile
HEALTHCHECK CMD ["btdt-server", "health-check", "https://localhost:8707/api/health"]
```

It is important to use the `CMD` form of the `HEALTHCHECK` instruction here, and not the `CMD-SHELL` form.
The latter would require a shell to be present in the container, which is not the case for the `btdt-server`
distroless image.

### Build from source using Rust

If you have Rust installed, you can build `btdt-server` from source using `cargo`:

```sh
cargo install btdt-server
```

## Configuration

The `btdt-server` is configured via a TOML configuration file at `/etc/btdt-server/config.toml`
(or `/config.toml` within the provided container image).

A minimal configuration should configure at least the location of the authorization private key and one cache location:

```toml
auth_private_key = "/etc/btdt-server/auth_private_key.pem"

[caches]
default = { type = 'Filesystem', path = '/var/lib/btdt/default' }
```

Note that the `auth_private_key` can alternatively also be set through the environment variable `BTDT_AUTH_PRIVATE_KEY`.

The URL to this cache location for `btdt` would be `http(s)://<btdt-server-host>:8707/api/caches/default`.

See [the configuration documentation](configuration.md) for more details on the available configuration options.

## Authorization

Authorization is done with [Eclipse Biscuit](https://www.biscuitsec.org/) tokens.
This avoids the need to manage user accounts on the server.
The server only needs to have a private key to verify the tokens.
If the private key is not present at server startup, a new key will be generated.

To generate the private key and derive authentication tokens, use the `biscuit` command line tool that can be
installed with

```sh
cargo install biscuit-cli
```

A new private key can be generated with

```sh
biscuit keypair --key-output-format pem --only-private-key | head -c -1 > auth_private_key.pem
```

To generate a new authorization token with all permissions and validity of 90 days, use

```sh
biscuit generate \
  --private-key-file auth_private_key.pem \
  --private-key-format pem \
  --add-ttl 90d - <<EOF
EOF
```

See [the authorization documentation](authorization.md) for more details.

## Enabling TLS

To enable TLS, simply provide a PKCS#12 file with the TLS certificate and private key.
Configure the path to this file with the `tls_keystore` option in the configuration file
or the `BTDT_TLS_KEYSTORE` environment variable. a password for the keystore can be provided
with the `tls_keystore_password` option or the `BTDT_TLS_KEYSTORE_PASSWORD` environment variable.

Note that, if you are using the `btdt-server` container image, you will have to adapt the health check command to:

```Dockerfile
HEALTHCHECK CMD ["btdt-server", "health-check", "https://localhost:8707/api/health"]
```

It is important to use the `CMD` form of the `HEALTHCHECK` instruction here, and not the `CMD-SHELL` form.
The latter would require a shell to be present in the container, which is not the case for the `btdt-server`
distroless image.
