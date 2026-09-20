use std::{
    fs::{self, OpenOptions},
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, params};
use safety::CapabilityProfile;
use secrecy::{ExposeSecret, SecretString};
use url::Url;
use uuid::Uuid;

use crate::{
    ConnectionMetadata, DecryptedConnection, TenantError, TenantGrant, UserId, UserRecord,
    crypto::EncryptionKey,
};

pub struct TenantStore {
    connection: Mutex<Connection>,
    encryption_key: EncryptionKey,
}

impl TenantStore {
    pub fn open(path: &Path, key_material: &str) -> Result<Self, TenantError> {
        let encryption_key = EncryptionKey::from_material(key_material)?;
        ensure_private_database_file(path)?;
        let connection = Connection::open(path).map_err(|_| TenantError::Storage)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE IF NOT EXISTS users (
                     id TEXT PRIMARY KEY NOT NULL,
                     github_id TEXT NOT NULL UNIQUE,
                     login TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS connections (
                     user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                     base_url TEXT NOT NULL,
                     profile INTEGER NOT NULL,
                     ciphertext BLOB NOT NULL,
                     created_at INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS grants (
                     id TEXT PRIMARY KEY NOT NULL,
                     user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                     client_id TEXT NOT NULL,
                     resource TEXT NOT NULL,
                     created_at INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS sessions (
                     id TEXT PRIMARY KEY NOT NULL,
                     user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
                     state_hash TEXT NOT NULL,
                     expires_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS grants_user_id_idx ON grants(user_id);
                 CREATE INDEX IF NOT EXISTS sessions_user_id_idx ON sessions(user_id);",
            )
            .map_err(|_| TenantError::Storage)?;

        Ok(Self {
            connection: Mutex::new(connection),
            encryption_key,
        })
    }

    pub fn upsert_user(&self, github_id: &str, login: &str) -> Result<UserRecord, TenantError> {
        let github_id = non_empty(github_id).ok_or(TenantError::InvalidIdentifier)?;
        let login = non_empty(login).ok_or(TenantError::InvalidIdentifier)?;
        let mut connection = self.lock_connection()?;
        let transaction = connection.transaction().map_err(|_| TenantError::Storage)?;

        let existing = transaction
            .query_row(
                "SELECT id FROM users WHERE github_id = ?1",
                [github_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| TenantError::Storage)?;
        let id = if let Some(id) = existing {
            let id = UserId::parse(&id).ok_or(TenantError::CorruptData)?;
            transaction
                .execute(
                    "UPDATE users SET login = ?1 WHERE id = ?2",
                    params![login, id.to_string()],
                )
                .map_err(|_| TenantError::Storage)?;
            id
        } else {
            let id = UserId::new();
            transaction
                .execute(
                    "INSERT INTO users (id, github_id, login) VALUES (?1, ?2, ?3)",
                    params![id.to_string(), github_id, login],
                )
                .map_err(|_| TenantError::Storage)?;
            id
        };

        transaction.commit().map_err(|_| TenantError::Storage)?;
        Ok(UserRecord {
            id,
            github_id: github_id.to_owned(),
            login: login.to_owned(),
        })
    }

    pub fn get_user_by_github_id(
        &self,
        github_id: &str,
    ) -> Result<Option<UserRecord>, TenantError> {
        let github_id = non_empty(github_id).ok_or(TenantError::InvalidIdentifier)?;
        let connection = self.lock_connection()?;
        connection
            .query_row(
                "SELECT id, github_id, login FROM users WHERE github_id = ?1",
                [github_id],
                |row| {
                    let id: String = row.get(0)?;
                    Ok((id, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
                },
            )
            .optional()
            .map_err(|_| TenantError::Storage)?
            .map(|(id, github_id, login)| {
                Ok(UserRecord {
                    id: UserId::parse(&id).ok_or(TenantError::CorruptData)?,
                    github_id,
                    login,
                })
            })
            .transpose()
    }

    pub fn save_connection(
        &self,
        user_id: UserId,
        base_url: &Url,
        token: &SecretString,
        profile: CapabilityProfile,
    ) -> Result<ConnectionMetadata, TenantError> {
        validate_url(base_url)?;
        if token.expose_secret().is_empty() {
            return Err(TenantError::InvalidToken);
        }
        let ciphertext = self.encryption_key.encrypt(user_id, token)?;
        let now = unix_timestamp();
        let mut connection = self.lock_connection()?;
        let transaction = connection.transaction().map_err(|_| TenantError::Storage)?;
        let user_exists = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id = ?1)",
                [user_id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| TenantError::Storage)?
            != 0;
        if !user_exists {
            return Err(TenantError::UserNotFound);
        }

        transaction
            .execute(
                "INSERT INTO connections
                    (user_id, base_url, profile, ciphertext, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(user_id) DO UPDATE SET
                    base_url = excluded.base_url,
                    profile = excluded.profile,
                    ciphertext = excluded.ciphertext,
                    updated_at = excluded.updated_at",
                params![
                    user_id.to_string(),
                    base_url.as_str(),
                    profile_to_i64(profile),
                    ciphertext,
                    now,
                ],
            )
            .map_err(|_| TenantError::Storage)?;
        transaction.commit().map_err(|_| TenantError::Storage)?;

        Ok(ConnectionMetadata {
            user_id,
            base_url: base_url.clone(),
            profile,
            updated_at: now,
        })
    }

    pub fn load_connection(
        &self,
        user_id: UserId,
    ) -> Result<Option<DecryptedConnection>, TenantError> {
        let connection = self.lock_connection()?;
        let row = connection
            .query_row(
                "SELECT base_url, profile, ciphertext FROM connections WHERE user_id = ?1",
                [user_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| TenantError::Storage)?;
        let Some((base_url, profile, ciphertext)) = row else {
            return Ok(None);
        };
        let base_url = Url::parse(&base_url).map_err(|_| TenantError::CorruptData)?;
        let profile = profile_from_i64(profile).ok_or(TenantError::CorruptData)?;
        let token = self.encryption_key.decrypt(user_id, &ciphertext)?;
        Ok(Some(DecryptedConnection {
            user_id,
            base_url,
            token,
            profile,
        }))
    }

    pub fn delete_connection(&self, user_id: UserId) -> Result<(), TenantError> {
        let mut connection = self.lock_connection()?;
        let transaction = connection.transaction().map_err(|_| TenantError::Storage)?;
        transaction
            .execute(
                "DELETE FROM connections WHERE user_id = ?1",
                [user_id.to_string()],
            )
            .map_err(|_| TenantError::Storage)?;
        transaction.commit().map_err(|_| TenantError::Storage)
    }

    pub fn create_grant(
        &self,
        user_id: UserId,
        client_id: &str,
        resource: &str,
    ) -> Result<TenantGrant, TenantError> {
        let client_id = non_empty(client_id).ok_or(TenantError::InvalidGrant)?;
        let resource = non_empty(resource).ok_or(TenantError::InvalidGrant)?;
        let grant_id = Uuid::new_v4().to_string();
        let created_at = unix_timestamp();
        let mut connection = self.lock_connection()?;
        let transaction = connection.transaction().map_err(|_| TenantError::Storage)?;
        let user_exists = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id = ?1)",
                [user_id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| TenantError::Storage)?
            != 0;
        if !user_exists {
            return Err(TenantError::UserNotFound);
        }
        transaction
            .execute(
                "INSERT INTO grants (id, user_id, client_id, resource, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    grant_id,
                    user_id.to_string(),
                    client_id,
                    resource,
                    created_at
                ],
            )
            .map_err(|_| TenantError::Storage)?;
        transaction.commit().map_err(|_| TenantError::Storage)?;

        Ok(TenantGrant {
            grant_id,
            user_id,
            client_id: client_id.to_owned(),
            resource: resource.to_owned(),
            created_at,
        })
    }

    pub fn revoke_user_grants(&self, user_id: UserId) -> Result<(), TenantError> {
        let mut connection = self.lock_connection()?;
        let transaction = connection.transaction().map_err(|_| TenantError::Storage)?;
        transaction
            .execute(
                "DELETE FROM grants WHERE user_id = ?1",
                [user_id.to_string()],
            )
            .map_err(|_| TenantError::Storage)?;
        transaction
            .execute(
                "DELETE FROM sessions WHERE user_id = ?1",
                [user_id.to_string()],
            )
            .map_err(|_| TenantError::Storage)?;
        transaction.commit().map_err(|_| TenantError::Storage)
    }

    fn lock_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, TenantError> {
        self.connection.lock().map_err(|_| TenantError::Storage)
    }
}

fn ensure_private_database_file(path: &Path) -> Result<(), TenantError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let metadata = fs::metadata(parent).map_err(|_| TenantError::Storage)?;
        if !metadata.is_dir() {
            return Err(TenantError::Storage);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o777 != 0o700 {
                return Err(TenantError::Storage);
            }
        }
    }

    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(|_| TenantError::Storage)?;
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|_| TenantError::Storage)?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions).map_err(|_| TenantError::Storage)?;
    }
    Ok(())
}

fn validate_url(url: &Url) -> Result<(), TenantError> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(TenantError::InvalidUrl);
    }
    Ok(())
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn profile_to_i64(profile: CapabilityProfile) -> i64 {
    match profile {
        CapabilityProfile::ReadOnly => 0,
        CapabilityProfile::Operations => 1,
        CapabilityProfile::Admin => 2,
    }
}

fn profile_from_i64(value: i64) -> Option<CapabilityProfile> {
    match value {
        0 => Some(CapabilityProfile::ReadOnly),
        1 => Some(CapabilityProfile::Operations),
        2 => Some(CapabilityProfile::Admin),
        _ => None,
    }
}
