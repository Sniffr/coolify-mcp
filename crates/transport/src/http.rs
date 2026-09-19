use oauth::OAuthProvider;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum UrlError {
    #[error("public URL must be HTTPS")]
    Insecure,
    #[error("invalid public URL")]
    Invalid,
}

pub fn normalize_public_url(raw: &str) -> Result<Url, UrlError> {
    let mut url = Url::parse(raw).map_err(|_| UrlError::Invalid)?;
    if url.scheme() != "https" {
        return Err(UrlError::Insecure);
    }
    if url.host_str().is_none() || url.query().is_some() || url.fragment().is_some() {
        return Err(UrlError::Invalid);
    }
    while url.path().ends_with('/') && url.path() != "/" {
        let _ = url.path_segments_mut().map(|mut p| {
            p.pop_if_empty();
        });
    }
    if url.path() == "/" {
        url.set_path("");
    }
    Ok(url)
}

#[derive(Clone)]
pub struct HttpConfig {
    pub public_url: Url,
    pub bind: SocketAddr,
    pub oauth: Arc<OAuthProvider>,
    pub max_body_bytes: usize,
    pub request_timeout: Duration,
    pub persistence_available: bool,
}
impl HttpConfig {
    pub fn for_tests() -> Self {
        let public_url = Url::parse("https://example.test").unwrap();
        Self {
            oauth: Arc::new(OAuthProvider::new(public_url.to_string(), "/mcp".into())),
            public_url,
            bind: "127.0.0.1:0".parse().unwrap(),
            max_body_bytes: 5 * 1024 * 1024,
            request_timeout: Duration::from_secs(30),
            persistence_available: true,
        }
    }
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("transport I/O error")]
    Io(#[from] std::io::Error),
    #[error("invalid public URL: {0}")]
    Url(#[from] UrlError),
}

pub async fn run_http<A: mcp_tools::McpApplication + 'static>(
    app: A,
    config: HttpConfig,
) -> Result<(), TransportError> {
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, crate::http_app::router_with_app(config, app))
        .await
        .map_err(|e| std::io::Error::other(e.to_string()).into())
}
