use config::builder::DefaultState;
use config::{Config, ConfigBuilder, ConfigError, Environment, File, Map, Source};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
pub struct BtdtServerConfig {
    pub bind_addrs: Vec<String>,
    pub enable_api_docs: bool,
    pub tls_keystore: String,
    pub tls_keystore_password: String,
    pub auth_private_key: String,

    pub cleanup: CleanupConfig,

    pub caches: HashMap<String, CacheConfig>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
pub struct CleanupConfig {
    pub interval: String,
    pub cache_expiration: String,
    pub max_cache_size: String,
}

impl BtdtServerConfig {
    pub fn load() -> Result<Self, LoadConfigError> {
        ConfigLoader::new().add_default_sources().load()
    }
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum CacheConfig {
    InMemory,
    Filesystem { path: String },
}

#[derive(Debug)]
pub enum LoadConfigError {
    ConfigError(ConfigError),
}

impl Display for LoadConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadConfigError::ConfigError(err) => write!(f, "configuration error: {err}"),
        }
    }
}

impl From<ConfigError> for LoadConfigError {
    fn from(err: ConfigError) -> Self {
        LoadConfigError::ConfigError(err)
    }
}

impl std::error::Error for LoadConfigError {}

struct ConfigLoader(ConfigBuilder<DefaultState>);

impl ConfigLoader {
    pub fn new() -> Self {
        ConfigLoader(Config::builder())
    }

    pub fn add_default_sources(self) -> Self {
        self.add_file_source(
            File::with_name(
                &std::env::var("BTDT_SERVER_CONFIG_FILE")
                    .map(Cow::Owned)
                    .unwrap_or(Cow::Borrowed("/etc/btdt-server/config.toml")),
            )
            .required(false),
        )
        .add_environment_source(None)
    }

    pub fn add_file_source<T, F>(mut self, file: File<T, F>) -> Self
    where
        File<T, F>: Source + Send + Sync + 'static,
    {
        self.0 = self.0.add_source(file);
        self
    }

    pub fn add_environment_source(mut self, source: Option<Map<String, String>>) -> Self {
        self.0 = self.0.add_source(
            Environment::with_prefix("BTDT")
                .separator("__")
                .prefix_separator("_")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("bind_addrs")
                .source(source),
        );
        self
    }

    pub fn load(self) -> Result<BtdtServerConfig, LoadConfigError> {
        self.0
            .set_default("bind_addrs", vec!["0.0.0.0:8707".to_string()])?
            .set_default("enable_api_docs", true)?
            .set_default("tls_keystore", "".to_string())?
            .set_default("tls_keystore_password", "".to_string())?
            .set_default("auth_private_key", "".to_string())?
            .set_default("cleanup.interval", "10min")?
            .set_default("cleanup.cache_expiration", "7days")?
            .set_default("cleanup.max_cache_size", "50GiB")?
            .set_default("caches", HashMap::<String, String>::new())?
            .build()?
            .try_deserialize()
            .map_err(LoadConfigError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::{File, FileFormat, Map};
    use std::collections::HashMap;

    #[test]
    fn test_configuration_defaults() {
        let default_config = ConfigLoader::new().load().unwrap();
        assert_eq!(
            default_config,
            BtdtServerConfig {
                bind_addrs: vec!["0.0.0.0:8707".to_string()],
                enable_api_docs: true,
                tls_keystore: "".to_string(),
                tls_keystore_password: "".to_string(),
                auth_private_key: "".to_string(),
                cleanup: CleanupConfig {
                    interval: "10min".to_string(),
                    cache_expiration: "7days".to_string(),
                    max_cache_size: "50GiB".to_string(),
                },
                caches: HashMap::new(),
            }
        )
    }

    #[test]
    fn test_parses_toml_configuration() {
        let config = "
            bind_addrs = ['127.0.0.1:8707', '[::1]:8707']
            enable_api_docs = false
            tls_keystore = 'path/certificate.p12'
            tls_keystore_password = 'password'
            auth_private_key = 'path/private-key'

            [cleanup]
            interval = '5min'
            cache_expiration = '14days'
            max_cache_size = '100GiB'

            [caches]
            in_memory = { type = 'InMemory' }
            filesystem = { type = 'Filesystem', path = '/var/lib/btdt-server/cache' }
        ";
        let file = File::from_str(config, FileFormat::Toml);
        let parsed_config = ConfigLoader::new().add_file_source(file).load().unwrap();
        assert_eq!(
            parsed_config,
            BtdtServerConfig {
                bind_addrs: vec!["127.0.0.1:8707".to_string(), "[::1]:8707".to_string()],
                enable_api_docs: false,
                tls_keystore: "path/certificate.p12".to_string(),
                tls_keystore_password: "password".to_string(),
                auth_private_key: "path/private-key".to_string(),
                cleanup: CleanupConfig {
                    interval: "5min".to_string(),
                    cache_expiration: "14days".to_string(),
                    max_cache_size: "100GiB".to_string(),
                },
                caches: HashMap::from([
                    ("in_memory".to_string(), CacheConfig::InMemory),
                    (
                        "filesystem".to_string(),
                        CacheConfig::Filesystem {
                            path: "/var/lib/btdt-server/cache".to_string()
                        }
                    )
                ])
            }
        );
    }

    #[test]
    fn test_parses_environment_variables() {
        let env = Map::from([
            (
                "BTDT_BIND_ADDRS".to_string(),
                "127.0.0.1:8707,[::1]:8707".to_string(),
            ),
            ("BTDT_ENABLE_API_DOCS".to_string(), "false".to_string()),
            (
                "BTDT_TLS_KEYSTORE".to_string(),
                "path/certificate.p12".to_string(),
            ),
            (
                "BTDT_TLS_KEYSTORE_PASSWORD".to_string(),
                "password".to_string(),
            ),
            (
                "BTDT_AUTH_PRIVATE_KEY".to_string(),
                "path/private-key".to_string(),
            ),
            ("BTDT_CLEANUP__INTERVAL".to_string(), "5min".to_string()),
            (
                "BTDT_CLEANUP__CACHE_EXPIRATION".to_string(),
                "14days".to_string(),
            ),
            (
                "BTDT_CLEANUP__MAX_CACHE_SIZE".to_string(),
                "100GiB".to_string(),
            ),
        ]);
        let parsed_config = ConfigLoader::new()
            .add_environment_source(Some(env))
            .load()
            .unwrap();
        assert_eq!(
            parsed_config,
            BtdtServerConfig {
                bind_addrs: vec!["127.0.0.1:8707".to_string(), "[::1]:8707".to_string()],
                enable_api_docs: false,
                tls_keystore: "path/certificate.p12".to_string(),
                tls_keystore_password: "password".to_string(),
                auth_private_key: "path/private-key".to_string(),
                cleanup: CleanupConfig {
                    interval: "5min".to_string(),
                    cache_expiration: "14days".to_string(),
                    max_cache_size: "100GiB".to_string(),
                },
                caches: HashMap::new(),
            }
        );
    }
}
