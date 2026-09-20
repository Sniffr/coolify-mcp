//! Server-side GitHub identity exchange for the hosted MCP transport.
mod github;

pub use github::{GitHubIdentityProvider, GitHubUser, IdentityError};
