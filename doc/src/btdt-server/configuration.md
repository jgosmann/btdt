# Configuration

The `btdt-server` is configured via a TOML configuration file at `/etc/btdt-server/config.toml`
(or `/config.toml` within the provided container image).
This location can be overridden by setting the `BTDT_SERVER_CONFIG_FILE` environment variable.
Some configuration options can also be set via environment variables, as described below.

## General options

### `auth_private_key`

- **Type:** string
- **Default:** `''`
- **Environment variable:** `BTDT_AUTH_PRIVATE_KEY`

Path to the [Eclipse Biscuit](https://www.biscuitsec.org/) private key file used to verify authorization tokens.
If the file does not exist at server startup, a new private key will be generated and saved to this location.
Note that the private key's permission must be restricted to `0600`.

### `bind_addrs`

- **Type:** array of strings
- **Default:** `['0.0.0.0:8707']`
- **Environment variable:** `BTDT_BIND_ADDRS`

List of addresses and ports the server should bind to.

### `enable_api_docs`

- **Type:** boolean
- **Default:** `true`
- **Environment variable:** `BTDT_ENABLE_API_DOCS`

If set to `true`, the server will provide API documentation at `/docs`.

### `tls_keystore`

- **Type:** string
- **Default:** `''`
- **Environment variable:** `BTDT_TLS_KEYSTORE`

Path to a PKCS#12 keystore file containing the TLS certificate and private key.
If not set, the server will run without TLS.

### `tls_keystore_password`

- **Type:** string
- **Default:** `''`
- **Environment variable:** `BTDT_TLS_KEYSTORE_PASSWORD`

Password for the PKCS#12 keystore file.

## Cleanup options

These options have to be set in the `[cleanup]` table.
They configure automatic cleanup of cached data to prevent indefinite growth of the cache storage.

### `cache_expiration`

- **Type:** duration string
- **Default:** `'7days'`
- **Environment variable:** `BTDT_CLEANUP__CACHE_EXPIRATION`

Caches that have not been accessed for this duration will be deleted during cleanup runs.

### `interval`

- **Type:** duration string
- **Default:** `'10min'`
- **Environment variable:** `BTDT_CLEANUP__INTERVAL`

Interval between cleanup runs.

### `max_cache_size`

- **Type:** size string
- **Default:** `'50GiB'`
- **Environment variable:** `BTDT_CLEANUP__MAX_CACHE_SIZE`

Maximum total size of each cache. Note that a cache might temporarily exceed this size between cleanup runs.

## Configuring caches

Caches are configured in the `[caches]` table.
Each table entry defines a cache with a unique name.
The cache base URL is derived form this name and of the form
`http(s)://<btdt-server-host>:8707/api/caches/<cache-name>`.

### Filesystem cache

A filesystem cache stores cached data in a directory on the local filesystem.

```toml
[caches]
my_cache = { type = 'Filesystem', path = '/var/lib/btdt/my_cache' }
```

### In-memory cache

An in-memory cache stores cached data in memory.
This cache is not persistent and will be lost when the server restarts.

> [!IMPORTANT]
> In-memory caches are intended for testing or development purposes.
> They are not optimized for speed and might not perform well for production workloads.

```toml
[caches]
my_cache = { type = 'InMemory' }
```

## Example configuration

```toml
bind_addrs = ['127.0.0.1:8707', '[::1]:8707']
enable_api_docs = false
tls_keystore = 'path/certificate.p12'
tls_keystore_password = 'password'
auth_private_key = 'path/private-key'

[cleanup]
interval = '5min'
cache_expiration = '14days'
max_cache_size = '100GiB'

[caches]
in_memory = { type = 'InMemory' }
filesystem = { type = 'Filesystem', path = '/var/lib/btdt-server/cache' }
```
