use config::builder::DefaultState;
use config::{Config, ConfigBuilder, ConfigError, Environment, File, Map, Source};
use std::fmt::{Debug, Display, Formatter};

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
pub struct BtdtServerConfig {
    pub bind_addrs: Vec<String>,
    pub enable_api_docs: bool,
    pub tls_keystore: String,
    pub tls_keystore_password: String,
}

impl BtdtServerConfig {
    pub fn load() -> Result<Self, LoadConfigError> {
        ConfigLoader::new().add_default_sources().load()
    }
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
        self.add_file_source(File::with_name("/etc/btdt-server").required(false))
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
            .build()?
            .try_deserialize()
            .map_err(LoadConfigError::from)
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{BtdtServerConfig, ConfigLoader};
    use config::{File, FileFormat, Map};

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
        ";
        let file = File::from_str(config, FileFormat::Toml);
        let parsed_config = ConfigLoader::new().add_file_source(file).load().unwrap();
        assert_eq!(
            parsed_config,
            BtdtServerConfig {
                bind_addrs: vec!["127.0.0.1:8707".to_string(), "[::1]:8707".to_string()],
                enable_api_docs: false,
                tls_keystore: "path/certificate.p12".to_string(),
                tls_keystore_password: "password".to_string()
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
            }
        );
    }
}
