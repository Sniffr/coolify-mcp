use std::fmt;

use safety::CapabilityProfile;
use secrecy::SecretString;
use url::Url;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UserId(Uuid);

impl UserId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Uuid::parse_str(value).ok().map(Self)
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserRecord {
    pub id: UserId,
    pub github_id: String,
    pub login: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionMetadata {
    pub user_id: UserId,
    pub base_url: Url,
    pub profile: CapabilityProfile,
    pub updated_at: i64,
}

#[derive(Debug)]
pub struct DecryptedConnection {
    pub user_id: UserId,
    pub base_url: Url,
    pub token: SecretString,
    pub profile: CapabilityProfile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TenantGrant {
    pub grant_id: String,
    pub user_id: UserId,
    pub client_id: String,
    pub resource: String,
    pub created_at: i64,
}
