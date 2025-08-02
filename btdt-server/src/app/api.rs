use poem_openapi::payload::PlainText;
use poem_openapi::{OpenApi, OpenApiService};

pub struct Api;

pub fn create_openapi_service() -> OpenApiService<Api, ()> {
    OpenApiService::new(Api, "btdt server API", "1.0")
}

#[OpenApi]
impl Api {
    /// Health check endpoint
    #[oai(path = "/health", method = "get")]
    async fn health(&self) -> PlainText<String> {
        PlainText("OK".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poem::http::StatusCode;
    use poem::test::TestClient;
    use poem::Route;

    #[tokio::test]
    async fn health_endpoint_returns_200() {
        let api_service = OpenApiService::new(Api, "btdt-server", "1.0");
        let app = Route::new().nest("/", api_service);
        let cli = TestClient::new(app);
        let resp = cli.get("/health").send().await;
        resp.assert_status(StatusCode::OK);
    }
}
