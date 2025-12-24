# CLI reference

The general syntax of the `btdt` command-line interface is:

```sh
btdt <SUBCOMMAND> [OPTIONS]
```

The available subcommands are described below.

## clean

```sh
btdt clean [OPTIONS] --cache <CACHE>
```

Clean old entries from a local cache.

### `-c <CACHE>`, `--cache <CACHE>`

Path to the local cache directory to clean.

### `--max-age <DURATION>`

Maximum age (e.g. `7d`, `48h`, `1d 12h`) of cache entries to keep. Cache entries not accessed within this duration will
be
deleted.

### `--max-size <SIZE>`

Maximum total size (e.g. `10GiB`, `500MB`) of the cache. If the cache exceeds this size, the least recently used caches
are deleted until the total size is below this limit.

## hash

```sh
btdt hash <PATH>
```

Calculate the hash of a file and print it to the standard output.

## help

```sh
btdt help [SUBCOMMAND]
```

Print general help or help for a specific subcommand.

## restore

```sh
btdt restore [OPTIONS] --keys <KEYS> --cache <CACHE> <DESTINATION_DIR>
```

Restore cached data from a cache to `<DESTINATION_DIR>`.
The first key that exists in the cache is used.

The result of the cache lookup is indicated via the exit code:

- `0`: Data was successfully restored from the cache using the primary (first listed) key.
- `1`: General error.
- `2`: Error in the command invocation or arguments.
- `3`: Files were **restored**, but not using the primary key (i.e., a fallback key was used).
- `4`: No cache entry found for any of the specified keys.

### `-a <AUTH_TOKEN_FILE>`, `--auth-token-file <AUTH_TOKEN_FILE>`

Path to a file containing the authentication token for accessing a remote cache.

> [!IMPORTANT]
> The file must be readable only by the user running `btdt`, i.e., it should have permissions `0600`.

### `-c <CACHE>`, `--cache <CACHE>`

Path to the cache (local directory or remote cache URL).

### `-k <KEYS>`, `--keys <KEYS>`

Comma-separated list of cache keys to try in order. This argument may also be repeated to specify multiple keys.

### `--root-cert <ROOT_CERT>`

Root certificates (in PEM format) to trust for remote caches (instead of system's root certificates).

### `--success-rc-on-any-key`

Exit with success status code if any key is found in the cache.

Usually, the success exit code is only returned if the primary key (i.e. first listed key) is found in the cache, and 3
is returned if another key was restored.

## store

```sh
btdt store [OPTIONS] --keys <KEYS> --cache <CACHE> <SOURCE_DIR>
```

Store data from `<SOURCE_DIR>` into the cache under the specified keys.

### `-a <AUTH_TOKEN_FILE>`, `--auth-token-file <AUTH_TOKEN_FILE>`

Path to a file containing the authentication token for accessing a remote cache.

> [!IMPORTANT]
> The file must be readable only by the user running `btdt`, i.e., it should have permissions `0600`.

### `-c <CACHE>`, `--cache <CACHE>`

Path to the cache (local directory or remote cache URL).

### `-k <KEYS>`, `--keys <KEYS>`

Comma-separated list of cache keys to store the cached data under.
This argument may also be repeated to specify multiple keys.

### `--root-cert <ROOT_CERT>`

Root certificates (in PEM format) to trust for remote caches (instead of system's root certificates).
