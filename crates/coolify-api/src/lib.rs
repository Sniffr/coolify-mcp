//! Typed Coolify API client foundation.

mod client;
mod config;
mod error;
mod token_source;

pub use client::CoolifyClient;
pub use config::{ConfigError, CoolifyConfig, config_from_env};
pub use error::{CoolifyApiError, HttpErrorDetails, MAX_BODY_BYTES};
pub use token_source::{TokenSource, TokenSourceError};
