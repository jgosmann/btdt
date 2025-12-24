# btdt: "been there, done that"

`btdt` is a tool for flexible caching of files in CI pipelines.
By being a simple CLI program, it is agnostic to the CI platform and can be integrated into various pipelines.

Cached data can either be stored locally in the filesystem, or remotely using the server component `btdt-server`.

**This tool is still under active development and not feature complete yet.**
See below for details.

## Example: caching `node_modules`

```sh
CACHE_KEY=node-modules-$(btdt hash package-lock.json)
btdt restore --cache path/to/cache --keys $CACHE_KEY node_modules
if [ $? -ne 0 ]; then
    npm ci
    btdt store --cache path/to/cache --keys $CACHE_KEY node_modules
fi
```

Examples for specific CI platforms can be found in the documentation (see below).

## Documentation

The main user guide and documentation is located at [https://jgosmann.github.io/btdt](https://jgosmann.github.io/btdt).
The API documentation is found on [docs.rs](https://docs.rs/btdt/latest/btdt/)

## Motivation

I was annoyed that there isn't a good (self-hosted) caching solution for Jenkins and Tekton, similar to the
cache for GitHub Actions.
Also, it seemed that it shouldn't be that hard to implement a caching solution.
So I put my money where my mouth is.
In particular, I didn't see any reason why caching should be tied to a specific CI platform by implementing it as a
plugin for that platform.
To me, it seems, that this problem is solvable with a CLI tool that can be integrated into any pipeline.

Regarding Jenkins, I know of two caching plugins and I have my quarrels with both of them:

- [Job Cacher](https://plugins.jenkins.io/jobcacher/) will delete the complete cache once it reaches the maximum size.
  This is inefficient and I prefer to delete least recently used caches until the limit is met. The plugin also does
  not share the cache between different build jobs which severely limits its usefulness in certain scenarios. We also
  had some other constraints that made it impossible to use this plugin, but a CLI tool could have been integrated
  easily.
- [jenkins-pipeline-cache-plugin](https://github.com/j3t/jenkins-pipeline-cache-plugin) requires S3 API compatible
  storage, which excludes some other use cases. It also doesn't seem to provide a way to limit the cache size.

Regarding
Tekton, [a few suggestions are made in their blog](https://tekton.dev/blog/2023/11/02/speeding-up-container-image-builds-in-tekton-pipelines/#caching-dependencies-on-a-persistent-disk).
But I don't think those are perfect:

- Caching layers in a container registry imposes a dependency order on your cached layers. This might be fine, if
  invalidating one cache/layer, implies that all subsequent caches/layers are also invalidated. But if you have two
  orthogonal caches, you must decide for an order, and always have one case where one of the caches might be invalidated
  needlessly.
- Caching on a persistent disk does not, as far as I understand, allow for multiple caches to be stored without
  additional tooling. If you have builds that require different caches, you might end up overwriting caches constantly.
  `btdt` provides tooling to have multiple separate caches.

## State of development

A basic version of `btdt` that can be used in most scenarios is working.
Missing features concern primarily covenience and ease of use:

- Compression of the cache (to reduce the amount of data transferred).
- Hashing multiple files in a stable way for the cache key.
- A templating system for cache keys, such that `btdt hash` doesn't need to be called,
  but a cache key in the form of `cache-key-${hashFiles('**/package-lock.json')}` can be used directly.
- Potentially, using S3 compatible APIs as storage backend.
