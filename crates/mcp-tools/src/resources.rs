#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub uri: String,
    pub description: String,
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
