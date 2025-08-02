use poem::Route;

mod api;

pub fn create_route() -> Route {
    Route::new().nest("/api", api::create_openapi_service())
}
