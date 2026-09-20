//! MCP stdio and Streamable HTTP transports.
pub mod http;
pub mod http_app;
pub mod settings;
pub mod stdio;
pub use http::{
    HttpConfig, TransportError, UrlError, mcp_resource_url, normalize_public_url,
    normalize_public_url_with_insecure, public_base, run_http,
};
pub use stdio::run_stdio;
