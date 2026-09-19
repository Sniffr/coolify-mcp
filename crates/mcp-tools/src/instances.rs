use coolify_api::{CoolifyClient, CoolifyConfig};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use url::Url;

#[derive(Clone)]
pub struct Instance {
    name: String,
    base_url: Url,
    pub client: Arc<CoolifyClient>,
}
impl Instance {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }
}
impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance")
            .field("name", &self.name)
            .finish()
    }
}

#[derive(Clone, Debug, Default)]
pub struct InstanceRegistry {
    entries: Vec<Instance>,
}
#[derive(Debug, thiserror::Error)]
pub enum InstanceRegistryError {
    #[error("invalid COOLIFY_INSTANCES configuration")]
    Invalid,
    #[error("unknown instance name")]
    Unknown,
    #[error("could not create instance client")]
    Client,
}
#[derive(Deserialize)]
struct RawInstance {
    name: String,
    url: String,
    token: String,
}
impl InstanceRegistry {
    pub fn new(names: Vec<String>) -> Self {
        Self {
            entries: names
                .into_iter()
                .map(|name| Instance {
                    name,
                    base_url: Url::parse("http://127.0.0.1").unwrap(),
                    client: Arc::new(
                        CoolifyClient::new(CoolifyConfig {
                            base_url: Url::parse("http://127.0.0.1").unwrap(),
                            token_source: coolify_api::TokenSource::from_env(&HashMap::from([(
                                String::from("COOLIFY_ACCESS_TOKEN"),
                                String::from("unconfigured"),
                            )]))
                            .unwrap(),
                            custom_headers: Default::default(),
                            timeout: std::time::Duration::from_secs(45),
                        })
                        .unwrap(),
                    ),
                })
                .collect(),
        }
    }
    pub fn from_json(raw: &str) -> Result<Self, InstanceRegistryError> {
        let values: Vec<RawInstance> = if raw.trim_start().starts_with('{') {
            let map: HashMap<String, RawInstanceWithoutName> =
                serde_json::from_str(raw).map_err(|_| InstanceRegistryError::Invalid)?;
            map.into_iter()
                .map(|(name, x)| RawInstance {
                    name,
                    url: x.url,
                    token: x.token,
                })
                .collect()
        } else {
            serde_json::from_str(raw).map_err(|_| InstanceRegistryError::Invalid)?
        };
        if values.is_empty() {
            return Err(InstanceRegistryError::Invalid);
        }
        let mut entries = Vec::with_capacity(values.len());
        for value in values {
            if value.name.trim().is_empty() || value.token.is_empty() {
                return Err(InstanceRegistryError::Invalid);
            }
            let url = Url::parse(&value.url).map_err(|_| InstanceRegistryError::Invalid)?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(InstanceRegistryError::Invalid);
            }
            let env = HashMap::from([(String::from("COOLIFY_ACCESS_TOKEN"), value.token)]);
            let source = coolify_api::TokenSource::from_env(&env)
                .map_err(|_| InstanceRegistryError::Invalid)?;
            let config = CoolifyConfig {
                base_url: url.clone(),
                token_source: source,
                custom_headers: Default::default(),
                timeout: std::time::Duration::from_secs(45),
            };
            let client = CoolifyClient::new(config).map_err(|_| InstanceRegistryError::Client)?;
            entries.push(Instance {
                name: value.name,
                base_url: url,
                client: Arc::new(client),
            });
        }
        Ok(Self { entries })
    }
    pub fn from_env() -> Result<Self, InstanceRegistryError> {
        Self::from_json(
            &std::env::var("COOLIFY_INSTANCES").map_err(|_| InstanceRegistryError::Invalid)?,
        )
    }
    pub fn all(&self) -> Vec<String> {
        self.entries.iter().map(|x| x.name.clone()).collect()
    }
    pub fn is_fleet(&self) -> bool {
        self.entries.len() > 1
    }
    pub fn get(&self, name: &str) -> bool {
        self.entries.iter().any(|x| x.name == name)
    }
    pub fn select(&self, name: &str) -> Result<Instance, InstanceRegistryError> {
        self.entries
            .iter()
            .find(|x| x.name == name)
            .cloned()
            .ok_or(InstanceRegistryError::Unknown)
    }
    pub fn projection(&self) -> Vec<serde_json::Value> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, item)| {
                serde_json::json!({
                    "name": item.name,
                    "base_url": item.base_url.as_str().trim_end_matches('/'),
                    "default": index == 0,
                    "configured": true,
                })
            })
            .collect()
    }
}
#[derive(Deserialize)]
struct RawInstanceWithoutName {
    url: String,
    token: String,
}
