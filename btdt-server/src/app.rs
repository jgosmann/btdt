use crate::config::CacheConfig;
use biscuit_auth::KeyPair;
use poem::Route;
use std::collections::HashMap;

mod api;
mod get_from_cache;

#[derive(Clone, Debug)]
pub struct Options {
    enable_api_docs: bool,
}

impl Options {
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::new()
    }
}

#[derive(Clone, Debug, Default)]
pub struct OptionsBuilder {
    enable_api_docs: bool,
}

impl OptionsBuilder {
    pub fn new() -> Self {
        OptionsBuilder {
            enable_api_docs: false,
        }
    }

    pub fn enable_api_docs(mut self, value: bool) -> Self {
        self.enable_api_docs = value;
        self
    }

    pub fn build(self) -> Options {
        Options {
            enable_api_docs: self.enable_api_docs,
        }
    }
}

pub fn create_route(
    options: Options,
    cache_config: &HashMap<String, CacheConfig>,
    auth_key_pair: KeyPair,
) -> Route {
    const API_PREFIX: &str = "/api";
    let api_service =
        api::create_openapi_service(cache_config, auth_key_pair).url_prefix(API_PREFIX);
    let mut route = Route::new();
    if options.enable_api_docs {
        let docs = api_service.swagger_ui();
        route = route.nest("/docs", docs)
    }
    route.nest(API_PREFIX, api_service)
}
