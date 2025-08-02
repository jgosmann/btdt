use poem::Route;

mod api;

pub fn create_route() -> Route {
    let api_service = api::create_openapi_service();
    let docs = api_service.swagger_ui();
    Route::new().nest("/api", api_service).nest("/docs", docs)
}
