use crate::app::get_from_cache::GetFromCacheResponse;
use crate::config::CacheConfig;
use biscuit_auth::builder_ext::AuthorizerExt;
use biscuit_auth::macros::authorizer;
use biscuit_auth::{Biscuit, KeyPair};
use btdt::cache::Cache;
use btdt::cache::cache_dispatcher::CacheDispatcher;
use btdt::cache::local::LocalCache;
use btdt::storage::filesystem::FilesystemStorage;
use btdt::storage::in_memory::InMemoryStorage;
use btdt::util::close::Close;
use poem::Body;
use poem::http::StatusCode;
use poem_openapi::auth::Bearer;
use poem_openapi::param::{Path, Query};
use poem_openapi::payload::{PlainText, Response};
use poem_openapi::{OpenApi, OpenApiService, SecurityScheme};
use std::collections::HashMap;
use tokio::task::spawn_blocking;
use tokio_util::io::SyncIoBridge;

pub struct Api {
    caches: HashMap<String, CacheDispatcher>,
    auth_key_pair: KeyPair,
}

pub fn create_openapi_service(
    caches: HashMap<String, CacheDispatcher>,
    auth_key_pair: KeyPair,
) -> OpenApiService<Api, ()> {
    OpenApiService::new(
        Api {
            caches,
            auth_key_pair,
        },
        "btdt server API",
        "0.1",
    )
}

enum Operation {
    GetFromCache,
    PutIntoCache,
}

impl Operation {
    fn as_str(&self) -> &str {
        match self {
            Operation::GetFromCache => "get",
            Operation::PutIntoCache => "put",
        }
    }
}

#[derive(SecurityScheme)]
#[oai(
    ty = "bearer",
    key_in = "header",
    key_name = "Authorization",
    bearer_format = "Biscuit"
)]
struct BiscuitBearerAuth(Bearer);

impl BiscuitBearerAuth {
    fn authorize(
        &self,
        operation: Operation,
        cache_id: &str,
        auth_key_pair: &KeyPair,
    ) -> Result<(), poem::Error> {
        let token = Biscuit::from_base64(&self.0.token, auth_key_pair.public()).map_err(|err| {
            poem::Error::from_string(
                format!("Failed to parse authorization token: {err}"),
                StatusCode::UNAUTHORIZED,
            )
        })?;

        let mut authorizer = authorizer!(
            r#"operation({operation}); cache({cache_id});"#,
            operation = operation.as_str(),
            cache_id = cache_id
        )
        .time()
        .allow_all()
        .build(&token)
        .expect("Failed to create authorizer");
        authorizer
            .authorize()
            .map_err(|_| poem::Error::from_string("Access forbidden", StatusCode::FORBIDDEN))?;

        Ok(())
    }
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
        auth: BiscuitBearerAuth,
    ) -> Result<GetFromCacheResponse, poem::Error> {
        auth.authorize(Operation::GetFromCache, &cache_id.0, &self.auth_key_pair)?;
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
        auth: BiscuitBearerAuth,
    ) -> Result<Response<()>, poem::Error> {
        auth.authorize(Operation::PutIntoCache, &cache_id, &self.auth_key_pair)?;
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
    use biscuit_auth::Biscuit;
    use biscuit_auth::macros::{biscuit, block};
    use poem::Route;
    use poem::http::StatusCode;
    use poem::test::TestClient;
    use poem::web::headers::Authorization;
    use poem::web::headers::authorization::Bearer;
    use poem_openapi::auth;
    use tempfile::tempdir;

    struct TestFixture {
        #[allow(unused)]
        tempdir: tempfile::TempDir,
        client: TestClient<Route>,
        auth_token: Biscuit,
    }

    impl Default for TestFixture {
        fn default() -> Self {
            let tempdir = tempdir().unwrap();
            let caches: HashMap<String, CacheDispatcher> = HashMap::from([(
                "test-cache".to_string(),
                CacheDispatcher::InMemory(LocalCache::new(InMemoryStorage::new())),
            )]);
            let auth_key_pair = KeyPair::new();
            let auth_token = biscuit!("").build(&auth_key_pair).unwrap();
            let api_service = OpenApiService::new(
                Api {
                    caches,
                    auth_key_pair,
                },
                "btdt-server",
                "1.0",
            );
            let app = Route::new().nest("/", api_service);
            TestFixture {
                tempdir,
                client: TestClient::new(app),
                auth_token,
            }
        }
    }

    trait BiscuitTestExt {
        fn to_header(&self) -> Authorization<Bearer>;
    }

    impl BiscuitTestExt for Biscuit {
        fn to_header(&self) -> Authorization<Bearer> {
            Authorization::bearer(&self.to_base64().unwrap()).unwrap()
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
            .typed_header(fixture.auth_token.to_header())
            .send()
            .await;
        resp.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_on_caches_endpoint_returns_401_without_authorization_token() {
        let fixture = TestFixture::default();
        let resp = fixture
            .client
            .get("/caches/test-cache")
            .query("key", &"some-key")
            .send()
            .await;
        resp.assert_status(StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn get_on_caches_endpoint_returns_403_without_required_permission() {
        let fixture = TestFixture::default();
        let attenuated_token = fixture
            .auth_token
            .append(block!(
                r#"check if operation({operation}); check if cache("other-cache");"#,
                operation = Operation::GetFromCache.as_str()
            ))
            .unwrap();
        let resp = fixture
            .client
            .get("/caches/test-cache")
            .query("key", &"some-key")
            .typed_header(attenuated_token.to_header())
            .send()
            .await;
        resp.assert_status(StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_on_caches_endpoint_returns_204_for_non_existent_keys() {
        let fixture = TestFixture::default();
        let resp = fixture
            .client
            .get("/caches/test-cache")
            .query("key", &"non-existent-0")
            .query("key", &"non-existent-1")
            .typed_header(fixture.auth_token.to_header())
            .send()
            .await;
        resp.assert_status(StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn put_on_caches_endpoint_returns_404_for_non_existent_repository() {
        let fixture = TestFixture::default();
        let resp = fixture
            .client
            .put("/caches/nonexistent")
            .query("key", &"some-key")
            .typed_header(fixture.auth_token.to_header())
            .send()
            .await;
        resp.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn put_on_caches_endpoint_returns_204() {
        let fixture = TestFixture::default();
        let resp = fixture
            .client
            .put("/caches/test-cache")
            .query("key", &"test-key")
            .typed_header(fixture.auth_token.to_header())
            .body("test-value")
            .send()
            .await;
        resp.assert_status(StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn put_on_caches_endpoint_returns_401_without_authorization_token() {
        let fixture = TestFixture::default();
        let resp = fixture
            .client
            .put("/caches/test-cache")
            .query("key", &"test-key")
            .body("test-value")
            .send()
            .await;
        resp.assert_status(StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn put_on_caches_endpoint_returns_403_without_required_permission() {
        let fixture = TestFixture::default();
        let attenuated_token = fixture
            .auth_token
            .append(block!(
                r#"check if operation({operation}); check if cache("other-cache");"#,
                operation = Operation::PutIntoCache.as_str()
            ))
            .unwrap();
        let resp = fixture
            .client
            .put("/caches/test-cache")
            .query("key", &"test-key")
            .typed_header(attenuated_token.to_header())
            .body("test-value")
            .send()
            .await;
        resp.assert_status(StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn roundtrip_caches_endpoint() {
        let fixture = TestFixture::default();
        let put_resp = fixture
            .client
            .put("/caches/test-cache")
            .query("key", &"test-key-0")
            .query("key", &"test-key-1")
            .typed_header(fixture.auth_token.to_header())
            .body("test-value")
            .send()
            .await;
        put_resp.assert_status(StatusCode::NO_CONTENT);

        let get_resp = fixture
            .client
            .get("/caches/test-cache")
            .query("key", &"test-key")
            .query("key", &"test-key-0")
            .typed_header(fixture.auth_token.to_header())
            .send()
            .await;
        get_resp.assert_status(StatusCode::OK);
        get_resp.assert_text("test-value").await;

        let get_resp = fixture
            .client
            .get("/caches/test-cache")
            .query("key", &"test-key")
            .query("key", &"test-key-1")
            .typed_header(fixture.auth_token.to_header())
            .send()
            .await;
        get_resp.assert_status(StatusCode::OK);
        get_resp.assert_text("test-value").await;
    }

    #[test]
    fn test_bearer_auth_all_operations_allowed_with_unattenuated_token() {
        let key_pair = KeyPair::new();
        let token = biscuit!("").build(&key_pair).unwrap();
        let auth = BiscuitBearerAuth(auth::Bearer {
            token: token.to_base64().unwrap(),
        });
        assert!(
            auth.authorize(Operation::GetFromCache, "some-cache", &key_pair)
                .is_ok()
        );
        assert!(
            auth.authorize(Operation::PutIntoCache, "some-cache", &key_pair)
                .is_ok()
        );
    }

    #[test]
    fn test_bearer_auth_allows_attenuating_put_operation() {
        let key_pair = KeyPair::new();
        let token = biscuit!(
            "check if operation({operation});",
            operation = Operation::GetFromCache.as_str()
        )
        .build(&key_pair)
        .unwrap();
        let auth = BiscuitBearerAuth(auth::Bearer {
            token: token.to_base64().unwrap(),
        });
        assert!(
            auth.authorize(Operation::GetFromCache, "some-cache", &key_pair)
                .is_ok()
        );
        assert!(
            auth.authorize(Operation::PutIntoCache, "some-cache", &key_pair)
                .is_err()
        );
    }

    #[test]
    fn test_bearer_auth_allows_attenuating_get_operation() {
        let key_pair = KeyPair::new();
        let token = biscuit!(
            "check if operation({operation});",
            operation = Operation::PutIntoCache.as_str()
        )
        .build(&key_pair)
        .unwrap();
        let auth = BiscuitBearerAuth(auth::Bearer {
            token: token.to_base64().unwrap(),
        });
        assert!(
            auth.authorize(Operation::PutIntoCache, "some-cache", &key_pair)
                .is_ok()
        );
        assert!(
            auth.authorize(Operation::GetFromCache, "some-cache", &key_pair)
                .is_err()
        );
    }

    #[test]
    fn test_bearer_auth_allows_attenuating_cache_id() {
        let key_pair = KeyPair::new();
        let token = biscuit!(r#"check if cache("access-granted");"#)
            .build(&key_pair)
            .unwrap();
        let auth = BiscuitBearerAuth(auth::Bearer {
            token: token.to_base64().unwrap(),
        });
        assert!(
            auth.authorize(Operation::GetFromCache, "access-granted", &key_pair)
                .is_ok()
        );
        assert!(
            auth.authorize(Operation::GetFromCache, "access-denied", &key_pair)
                .is_err()
        );
    }

    #[test]
    fn test_bearer_auth_allows_time_limit_on_token() {
        let key_pair = KeyPair::new();

        let expired_token = biscuit!(r#"check if time($time), $time <= 1970-01-01T00:00:00Z;"#)
            .build(&key_pair)
            .unwrap();
        let auth = BiscuitBearerAuth(auth::Bearer {
            token: expired_token.to_base64().unwrap(),
        });
        assert!(
            auth.authorize(Operation::GetFromCache, "cache-id", &key_pair)
                .is_err()
        );

        let fresh_token = biscuit!(r#"check if time($time), $time <= 9999-12-31T23:59:59Z;"#)
            .build(&key_pair)
            .unwrap();
        let auth = BiscuitBearerAuth(auth::Bearer {
            token: fresh_token.to_base64().unwrap(),
        });
        assert!(
            auth.authorize(Operation::GetFromCache, "cache-id", &key_pair)
                .is_ok()
        );
    }
}
