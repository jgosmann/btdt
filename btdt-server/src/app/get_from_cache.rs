use crate::app::asyncio::StreamAdapter;
use btdt::cache::CacheHit;
use poem::Body;
use poem_openapi::ApiResponse;
use poem_openapi::payload::Binary;
use std::io::Read;

#[derive(ApiResponse)]
#[allow(clippy::enum_variant_names)]
pub enum GetFromCacheResponse {
    /// No matching key was found in the cache.
    #[oai(status = 204)]
    CacheMiss,
    /// The cache with the given ID does not exist.
    #[oai(status = 404)]
    CacheNotFound,
    /// The data was found in the cache and is returned as a binary response.
    #[oai(status = 200)]
    CacheHit(Binary<Body>),
}

impl<'a, R> From<CacheHit<'a, R>> for GetFromCacheResponse
where
    R: Read + Send + 'static,
{
    fn from(hit: CacheHit<R>) -> Self {
        GetFromCacheResponse::CacheHit(Binary(Body::from_bytes_stream(StreamAdapter::new(
            Box::new(hit.reader),
        ))))
    }
}
