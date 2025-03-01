# Getting Started

This guide will show you the general steps of using `btdt` to cache files, in particular,
installed dependencies of a package manager such as `npm`.
If you are looking to integrate `btdt` into a CI pipeline,
you might want to also check out the [CI-specific integration guides](./ci-guides/overview.md).

## Determining cache keys

Usually, you will have a file that completely specifies the dependencies and their versions of your project.
For example, in the JavaScript/NPM ecosystem, this is the `package-lock.json` file.
As long as it doesn't change, the installed dependencies will be the same and could be cached.
Thus, the primary cache key should be based on this file.

We can use the `btdt hash` command to generate a cache key from the file:

```sh
CACHE_KEY=cache-key-$(btdt hash package-lock.json)
```

This calculates a cryptographic hash over the file contents and appends it to the string `cache-key-`.
The result could look something like `cache-key-f3dd7a501dd93486194e752557585a1996846b9a6df16e76f104e81192b0039f`.
If the `package-lock.json` file changes, the hash will change as well and the cache key will be different.

## Trying to restore the cache

Before we try to install the dependencies, e.g. with `npm ci`, we can try to restore the cache instead:

```sh
btdt restore --cache path/to/cache --keys $CACHE_KEY node_modules
RESTORE_EXIT_CODE=$?
```

`npm` will install the dependencies into `node_modules`, so we are using this as the target directory.
Furthermore, we will store the exit code because it comes in handy in the next step. It will be `0` if the cache was
restored successfully from the first given key, and non-zero otherwise. (Use the `--success-rc-on-any-key` flag to
return a zero exit code no matter the key that was used to restore the cache.)

## Installing dependencies and storing the cache

If the cache could not be restored, we will install the dependencies with `npm ci`, and then store the installed
dependencies in the cache:

```sh
if [ $RESTORE_EXIT_CODE -ne 0 ]; then
    npm ci  # Install dependencies
    btdt store --cache path/to/cache --keys $CACHE_KEY node_modules
fi
```

## Using multiple cache keys

You can specify multiple cache keys. This allows to have a fallback mechanism. The cache keys will be tried in order
during the restore operation and allow you to use a cache which might not contain the exact dependencies required, but
could still speed up the installation if most of them are contained.

With `npm` the usage of multiple cache keys could look like this:

```sh
CACHE_KEY=cache-key-$(btdt hash package-lock.json)

btdt restore --cache path/to/cache --keys "$CACHE_KEY,fallback" node_modules
RESTORE_EXIT_CODE=$?

npm ci

if [ $RESTORE_EXIT_CODE -ne 0 ]; then
    btdt store --cache path/to/cache --keys $CACHE_KEY,fallback node_modules
fi
```

This will store the latest cached dependencies also under the key `fallback`. This cache entry will be used, if no more
specific cache enry is found.

## Cleanup

To prevent the cache from growing indefinitely, you might want to clean up old cache entries from time to time, for
example to only keep cache entries accessed within the last seven days and limit the cache size to at most 10 GiB:

```sh
btdt clean --cache path/to/cache --max-age 7d --max-size 10GiB
```
