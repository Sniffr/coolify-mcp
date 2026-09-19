use coolify_api::CoolifyClient;
use reqwest::Method;
use safety::frame_untrusted;

#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub uri: String,
    pub description: String,
    pub read_only: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedResource {
    pub uri: String,
    pub read_only: bool,
    pub uuid: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceContent {
    pub uri: String,
    pub text: String,
    pub read_only: bool,
}
#[derive(Clone, Debug, Default)]
pub struct ResourceRegistry {
    resources: Vec<Resource>,
}
impl ResourceRegistry {
    pub fn get(&self, uri: &str) -> Option<&Resource> {
        self.resources.iter().find(|x| x.uri == uri)
    }
    pub fn all(&self) -> &[Resource] {
        &self.resources
    }
    pub fn resolve(&self, uri: &str) -> Option<ResolvedResource> {
        if uri == "coolify://overview" {
            return Some(ResolvedResource {
                uri: uri.into(),
                read_only: true,
                uuid: None,
            });
        }
        let uuid = uri
            .strip_prefix("coolify://application/")
            .filter(|x| !x.is_empty() && !x.contains('/'))?;
        Some(ResolvedResource {
            uri: uri.into(),
            read_only: true,
            uuid: Some(uuid.into()),
        })
    }
    pub async fn read(&self, client: &CoolifyClient, uri: &str) -> Result<ResourceContent, String> {
        let resolved = self
            .resolve(uri)
            .ok_or_else(|| "resource is not registered".to_owned())?;
        let value = match resolved.uuid.as_deref() {
            Some(uuid) => client
                .get_application(uuid)
                .await
                .map(|v| serde_json::to_value(v).unwrap_or_default())
                .map_err(|e| e.to_string())?,
            None => client
                .request_value(Method::GET, "/resources", None)
                .await
                .map_err(|e| e.to_string())?,
        };
        let raw = serde_json::to_string(&value).map_err(|e| e.to_string())?;
        let bounded = raw.chars().take(200_000).collect::<String>();
        Ok(ResourceContent {
            uri: uri.into(),
            text: frame_untrusted(&bounded, "resource"),
            read_only: true,
        })
    }
}
pub fn register_resources() -> ResourceRegistry {
    ResourceRegistry {
        resources: vec![
            Resource {
                uri: "coolify://overview".into(),
                description: "Read-only estate overview projection".into(),
                read_only: true,
            },
            Resource {
                uri: "coolify://application/{uuid}".into(),
                description: "Read-only application projection".into(),
                read_only: true,
            },
        ],
    }
}
