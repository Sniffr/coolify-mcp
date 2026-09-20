//! Persistent, tenant-scoped storage for hosted Coolify connections.

mod crypto;
mod model;
mod store;

pub use model::{ConnectionMetadata, DecryptedConnection, TenantGrant, UserId, UserRecord};
pub use store::TenantStore;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TenantError {
    #[error("tenant encryption key is invalid")]
    InvalidKey,
    #[error("tenant identifier is invalid")]
    InvalidIdentifier,
    #[error("tenant storage is unavailable")]
    Storage,
    #[error("tenant record was not found")]
    UserNotFound,
    #[error("tenant connection URL is invalid")]
    InvalidUrl,
    #[error("tenant connection token is invalid")]
    InvalidToken,
    #[error("tenant grant is invalid")]
    InvalidGrant,
    #[error("tenant connection could not be decrypted")]
    Decryption,
    #[error("tenant encryption failed")]
    Crypto,
    #[error("tenant data is corrupt")]
    CorruptData,
}
