use oauth::OAuthProvider;
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use thiserror::Error;
use tower::Service;
use url::Url;

#[derive(Debug, Error)]
pub enum UrlError {
    #[error("public URL must be HTTPS")]
    Insecure,
    #[error("invalid public URL")]
    Invalid,
}
pub fn normalize_public_url(raw: &str) -> Result<Url, UrlError> {
    normalize_public_url_with_insecure(raw, false)
}

/// Base public URL without a trailing slash, so endpoint concatenation never
/// produces a double slash (which would fail OAuth resource comparison).
pub fn public_base(public_url: &Url) -> String {
    public_url.as_str().trim_end_matches('/').to_owned()
}
/// Canonical `…/mcp` resource identifier bound to OAuth tokens.
pub fn mcp_resource_url(public_url: &Url) -> String {
    format!("{}/mcp", public_base(public_url))
}
/// Normalize a public URL, allowing plain HTTP only for explicitly local acceptance runs.
pub fn normalize_public_url_with_insecure(
    raw: &str,
    allow_insecure: bool,
) -> Result<Url, UrlError> {
    let mut url = Url::parse(raw).map_err(|_| UrlError::Invalid)?;
    if url.scheme() != "https" && !(allow_insecure && url.scheme() == "http") {
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
    pub header_timeout: Duration,
    pub request_timeout: Duration,
    pub persistence_available: bool,
    pub persistence_health: Arc<AtomicBool>,
    pub max_sessions: usize,
    pub session_ttl: Duration,
    pub trusted_proxy: bool,
}
impl HttpConfig {
    pub fn for_tests() -> Self {
        let public_url = Url::parse("https://example.test").unwrap();
        Self {
            oauth: Arc::new(OAuthProvider::new(public_base(&public_url), "/mcp".into())),
            public_url,
            bind: "127.0.0.1:0".parse().unwrap(),
            max_body_bytes: 5 * 1024 * 1024,
            header_timeout: Duration::from_secs(15),
            request_timeout: Duration::from_secs(30),
            persistence_available: true,
            persistence_health: Arc::new(AtomicBool::new(true)),
            max_sessions: 1024,
            session_ttl: Duration::from_secs(3600),
            trusted_proxy: false,
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
pub const HTTP2_SUPPORTED: bool = false;
pub const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
pub async fn run_http<A: mcp_tools::McpApplication + 'static>(
    app: A,
    config: HttpConfig,
) -> Result<(), TransportError> {
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let router = crate::http_app::router_with_app(config.clone(), app);
    let mut shutdown = Box::pin(shutdown_signal());
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {_=&mut shutdown=>break,Some(_)=connections.join_next(),if !connections.is_empty()=>{},accepted=listener.accept()=>{let (stream,peer)=accepted?;let mut make=router.clone().into_make_service_with_connect_info::<SocketAddr>();let service=match make.call(peer).await{Ok(s)=>s,Err(_)=>continue};let timeout=config.header_timeout;connections.spawn(async move{let io=hyper_util::rt::TokioIo::new(stream);let mut builder=hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new()).http1_only(); builder.http1().timer(hyper_util::rt::TokioTimer::new()); builder.http1().header_read_timeout(timeout);let _=builder.serve_connection_with_upgrades(io,hyper_util::service::TowerToHyperService::new(service)).await;});}}
    }
    let drain = async { while connections.join_next().await.is_some() {} };
    let _ = tokio::time::timeout(DRAIN_TIMEOUT, drain).await;
    if !connections.is_empty() {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    if config.oauth.flush().is_err() {
        config.persistence_health.store(false, Ordering::Release);
    }
    Ok(())
}
