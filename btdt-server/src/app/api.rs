use crate::app::get_from_cache::GetFromCacheResponse;
use crate::config::CacheConfig;
use btdt::cache::Cache;
use btdt::cache::cache_dispatcher::CacheDispatcher;
use btdt::cache::local::LocalCache;
use btdt::storage::filesystem::FilesystemStorage;
use btdt::storage::in_memory::InMemoryStorage;
use btdt::util::close::Close;
use poem::Body;
use poem::http::StatusCode;
use poem_openapi::param::{Path, Query};
use poem_openapi::payload::{PlainText, Response};
use poem_openapi::{OpenApi, OpenApiService};
use std::collections::HashMap;
use tokio::task::spawn_blocking;
use tokio_util::io::SyncIoBridge;

pub struct Api {
    caches: HashMap<String, CacheDispatcher>,
}

pub fn create_openapi_service(config: &HashMap<String, CacheConfig>) -> OpenApiService<Api, ()> {
    let caches = config
        .iter()
        .map(|(key, cache_config)| {
            (
                key.clone(),
                match cache_config {
                    CacheConfig::InMemory => {
                        CacheDispatcher::InMemory(LocalCache::new(InMemoryStorage::new()))
                    }
                    CacheConfig::Filesystem { path } => CacheDispatcher::Filesystem(
                        LocalCache::new(FilesystemStorage::new(path.into())),
                    ),
                },
            )
        })
        .collect();
    OpenApiService::new(Api { caches }, "btdt server API", "0.1")
}

#[OpenApi]
impl Api {
    /// Health check endpoint
    ///
    /// Returns a simple "OK" response to indicate that the server is running.
    #[oai(path = "/health", method = "get")]
    async fn health(&self) -> PlainText<String> {
        PlainText("OK".to_string())
    }

    /// Returns the data stored under the first given key found in the cache. If none
    /// of the keys is found, 204 "no content" is returned.
    #[oai(path = "/caches/:cache_id", method = "get")]
    async fn get_from_cache(
        &self,
        cache_id: Path<String>,
        key: Query<Vec<String>>,
    ) -> Result<GetFromCacheResponse, poem::Error> {
        Ok(match self.caches.get(&cache_id.0) {
            Some(cache) => {
                match cache
                    .get(&key.0.iter().map(String::as_ref).collect::<Vec<_>>())
                    .map_err(poem::error::InternalServerError)?
                {
                    None => GetFromCacheResponse::CacheMiss,
                    Some(cache_hit) => cache_hit.into(),
                }
            }
            None => GetFromCacheResponse::CacheNotFound,
        })
    }

    /// Stores the data under all the given keys in the cache.
    #[oai(path = "/caches/:cache_id", method = "put")]
    async fn put_into_cache(
        &self,
        cache_id: Path<String>,
        key: Query<Vec<String>>,
        body: Body,
    ) -> Result<Response<()>, poem::Error> {
        Ok(match self.caches.get(&cache_id.0) {
            Some(cache) => {
                let mut writer = cache
                    .set(&key.0.iter().map(String::as_ref).collect::<Vec<_>>())
                    .map_err(poem::error::InternalServerError)?;
                let mut sync_reader = SyncIoBridge::new(body.into_async_read());
                spawn_blocking(move || {
                    std::io::copy(&mut sync_reader, &mut writer)?;
                    writer.close()
                })
                .await
                .map_err(poem::error::InternalServerError)?
                .map_err(poem::error::InternalServerError)?;
                Response::new(()).status(StatusCode::NO_CONTENT)
            }
            None => Response::new(()).status(StatusCode::NOT_FOUND),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poem::Route;
    use poem::http::StatusCode;
    use poem::test::TestClient;
    use tempfile::tempdir;

    struct TestFixture {
        #[allow(unused)]
        tempdir: tempfile::TempDir,
        client: TestClient<Route>,
    }

    impl Default for TestFixture {
        fn default() -> Self {
            let tempdir = tempdir().unwrap();
            let caches: HashMap<String, CacheDispatcher> = HashMap::from([(
                "test-cache".to_string(),
                CacheDispatcher::InMemory(LocalCache::new(InMemoryStorage::new())),
            )]);
            let api_service = OpenApiService::new(Api { caches }, "btdt-server", "1.0");
            let app = Route::new().nest("/", api_service);
            TestFixture {
                tempdir,
                client: TestClient::new(app),
            }
        }
    }

    #[tokio::test]
    async fn health_endpoint_returns_200() {
        let fixture = TestFixture::default();
        let resp = fixture.client.get("/health").send().await;
        resp.assert_status(StatusCode::OK);
    }

    #[tokio::test]
    async fn get_on_caches_endpoint_returns_404_for_non_existent_repository() {
        let fixture = TestFixture::default();
        let resp = fixture
            .client
            .get("/caches/nonexistent")
            .query("key", &"some-key")
            .send()
            .await;
        resp.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_on_caches_endpoint_returns_404_for_non_existent_repository() {
        let fixture = TestFixture::default();
        let resp = fixture
            .client
            .put("/caches/nonexistent")
            .query("key", &"some-key")
            .send()
            .await;
        resp.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_on_caches_endpoint_returns_204_for_non_existent_keys() {
        let fixture = TestFixture::default();
        let resp = fixture
            .client
            .get("/caches/test-cache")
            .query("key", &"non-existent-0")
            .query("key", &"non-existent-1")
            .send()
            .await;
        resp.assert_status(StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn put_on_caches_endpoint_returns_204() {
        let fixture = TestFixture::default();
        let resp = fixture
            .client
            .put("/caches/test-cache")
            .query("key", &"test-key")
            .body("test-value")
            .send()
            .await;
        resp.assert_status(StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn roundtrip_caches_endpoint() {
        let fixture = TestFixture::default();
        let put_resp = fixture
            .client
            .put("/caches/test-cache")
            .query("key", &"test-key-0")
            .query("key", &"test-key-1")
            .body("test-value")
            .send()
            .await;
        put_resp.assert_status(StatusCode::NO_CONTENT);

        let get_resp = fixture
            .client
            .get("/caches/test-cache")
            .query("key", &"test-key")
            .query("key", &"test-key-0")
            .send()
            .await;
        get_resp.assert_status(StatusCode::OK);
        get_resp.assert_text("test-value").await;

        let get_resp = fixture
            .client
            .get("/caches/test-cache")
            .query("key", &"test-key")
            .query("key", &"test-key-1")
            .send()
            .await;
        get_resp.assert_status(StatusCode::OK);
        get_resp.assert_text("test-value").await;
    }
}
