use crate::http::HttpConfig;
use crate::settings::{parse_profile, profile_name, settings_html, validate_base_url};
use axum::{
    Router,
    body::Bytes,
    extract::{ConnectInfo, DefaultBodyLimit, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Json, Redirect, Response},
    routing::{get, post},
};
use coolify_api::{CoolifyClient, CoolifyConfig, TokenSource};
use mcp_tools::{McpApplication, TenantRequestContext, TenantToolContext, ToolResult};
use oauth::{AuthorizeRequest, OAuthError, RegistrationRequest, TokenRequest};
use safety::{AuditEvent, AuditLogger};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tenant::UserId;
use tower_http::timeout::{RequestBodyTimeoutLayer, TimeoutLayer};

#[derive(Clone)]
struct RateLimiter {
    entries: Arc<Mutex<HashMap<String, (Instant, u32)>>>,
    max_entries: usize,
    max_requests: u32,
    window: Duration,
}
impl RateLimiter {
    fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            max_entries: 1024,
            max_requests: 30,
            window: Duration::from_secs(60),
        }
    }
    fn allow(&self, key: String) -> bool {
        let mut e = self.entries.lock().unwrap();
        let now = Instant::now();
        if e.len() >= self.max_entries && !e.contains_key(&key) {
            e.retain(|_, (at, _)| now.duration_since(*at) < self.window);
            if e.len() >= self.max_entries {
                return false;
            }
        }
        let slot = e.entry(key).or_insert((now, 0));
        if now.duration_since(slot.0) >= self.window {
            *slot = (now, 0)
        };
        slot.1 += 1;
        slot.1 <= self.max_requests
    }
}
#[derive(Clone)]
struct PendingAuthorization {
    request: AuthorizeRequest,
    client_state: String,
    expires_at: Instant,
}
#[derive(Serialize, Deserialize)]
struct PendingAuthorizationRecord {
    request: AuthorizeRequest,
    client_state: String,
}

#[derive(Clone)]
struct GithubContinuation {
    mcp_state: String,
    expires_at: Instant,
}

#[derive(Default)]
struct BrowserState {
    pending: HashMap<String, PendingAuthorization>,
    github: HashMap<String, GithubContinuation>,
    sessions: HashMap<String, (UserId, Instant, String)>,
}

#[derive(Clone)]
struct AppState {
    app: Arc<dyn McpApplication>,
    config: HttpConfig,
    rate: RateLimiter,
    sessions: Arc<Mutex<HashMap<String, (String, Instant)>>>,
    browser: Arc<Mutex<BrowserState>>,
    audit: Option<Arc<Mutex<AuditLogger<File>>>>,
}
fn open_audit(config: &HttpConfig) -> Option<Arc<Mutex<AuditLogger<File>>>> {
    if let Some(parent) = config.audit_path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    let previous_hash = File::open(&config.audit_path)
        .ok()
        .and_then(|file| {
            BufReader::new(file)
                .lines()
                .map_while(Result::ok)
                .last()
                .and_then(|line| serde_json::from_str::<Value>(&line).ok())
                .and_then(|record| {
                    record
                        .get("hash")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
        })
        .unwrap_or_default();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.audit_path)
        .ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ =
            std::fs::set_permissions(&config.audit_path, std::fs::Permissions::from_mode(0o600));
    }
    Some(Arc::new(Mutex::new(AuditLogger::with_previous_hash(
        file,
        previous_hash,
    ))))
}

fn audit_event(
    state: &AppState,
    client_id: Option<&str>,
    tool: &str,
    args: Option<&Value>,
    outcome: &str,
    status: StatusCode,
    started: Instant,
) {
    let Some(logger) = &state.audit else { return };
    let resource_id = args
        .and_then(|v| v.get("uuid").or_else(|| v.get("id")))
        .and_then(Value::as_str)
        .map(|v| v.chars().take(128).collect::<String>())
        .unwrap_or_default();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let event = AuditEvent::new(
        timestamp,
        client_id,
        tool.chars().take(128).collect::<String>(),
        resource_id,
        outcome,
        status.as_u16(),
        started.elapsed().as_millis().min(u64::MAX as u128) as u64,
    );
    let _ = logger.lock().unwrap().record(event);
}

struct EmptyApp;
impl McpApplication for EmptyApp {
    fn tools(&self) -> Vec<mcp_tools::ToolSpec> {
        vec![]
    }
    fn call<'a>(
        &'a self,
        _: &'a str,
        _: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
        Box::pin(async {
            ToolResult {
                text: "{}".into(),
                is_error: false,
            }
        })
    }
}
pub fn router(config: HttpConfig) -> Router {
    router_with_app(config, EmptyApp).layer(axum::Extension(ConnectInfo(
        "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
    )))
}
pub fn router_with_app<A: McpApplication + 'static>(config: HttpConfig, app: A) -> Router {
    let audit = open_audit(&config);
    if audit.is_none() {
        config
            .persistence_health
            .store(false, std::sync::atomic::Ordering::Release);
    }
    let state = AppState {
        app: Arc::new(app),
        config,
        rate: RateLimiter::new(),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        browser: Arc::new(Mutex::new(BrowserState::default())),
        audit,
    };
    if let Some(hosted) = state.config.hosted_auth.as_ref()
        && hosted.tenant.purge_expired_sessions().is_err()
    {
        state
            .config
            .persistence_health
            .store(false, std::sync::atomic::Ordering::Release);
    }
    let request_timeout = state.config.request_timeout;
    Router::new()
        .route("/healthz", get(health))
        .route("/.well-known/oauth-authorization-server", get(discovery))
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route("/oauth/register", post(register))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/state", post(oauth_state))
        .route("/oauth/token", post(token))
        .route("/auth/github/start", get(github_start))
        .route("/auth/github/callback", get(github_callback))
        .route("/settings", get(settings_get))
        .route(
            "/settings/coolify",
            post(settings_save).delete(settings_delete),
        )
        .route("/settings/logout", post(settings_logout))
        .route("/mcp", post(mcp).get(mcp_get).delete(mcp_delete))
        .layer(DefaultBodyLimit::max(state.config.max_body_bytes))
        .layer(RequestBodyTimeoutLayer::new(request_timeout))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            request_timeout,
        ))
        .with_state(state)
}
async fn health(State(s): State<AppState>) -> Response {
    let healthy = s.config.persistence_available
        && s.config
            .persistence_health
            .load(std::sync::atomic::Ordering::Acquire);
    let status = if healthy { "ok" } else { "degraded" };
    let code = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(json!({"status":status,"persistence":healthy}))).into_response()
}
async fn discovery(State(s): State<AppState>) -> impl IntoResponse {
    Json(
        json!({"issuer":crate::http::public_base(&s.config.public_url),"authorization_endpoint":format!("{}/oauth/authorize",crate::http::public_base(&s.config.public_url)),"token_endpoint":format!("{}/oauth/token",crate::http::public_base(&s.config.public_url)),"registration_endpoint":format!("{}/oauth/register",crate::http::public_base(&s.config.public_url)),"response_types_supported":["code"],"code_challenge_methods_supported":["S256"]}),
    )
}
async fn protected_resource(State(s): State<AppState>) -> impl IntoResponse {
    Json(
        json!({"resource":crate::http::mcp_resource_url(&s.config.public_url),"authorization_servers":[crate::http::public_base(&s.config.public_url)]}),
    )
}
fn client_key(s: &AppState, headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    if s.config.trusted_proxy
        && let Some(value) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
    {
        return value.chars().take(128).collect();
    }
    peer.map(|address| address.ip().to_string())
        .unwrap_or_else(|| "unknown-peer".into())
}
fn limited(s: &AppState, headers: &HeaderMap, peer: Option<SocketAddr>, endpoint: &str) -> bool {
    s.rate
        .allow(format!("{endpoint}:{}", client_key(s, headers, peer)))
}
async fn register(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Json(req): Json<RegistrationRequest>,
) -> Response {
    if !persistence_ready(&s) {
        return persistence_unavailable();
    }
    if !limited(&s, &headers, Some(peer.0), "register") {
        return rate_error();
    }
    match s.config.oauth.register(req) {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => {
            mark_persistence_error(&s, &e);
            oauth_error(e)
        }
    }
}
#[derive(Deserialize)]
struct OAuthStateRequest {
    client_id: String,
    redirect_uri: String,
}
async fn oauth_state(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Json(req): Json<OAuthStateRequest>,
) -> Response {
    if !persistence_ready(&s) {
        return persistence_unavailable();
    }
    if !limited(&s, &headers, Some(peer.0), "authorize") {
        return rate_error();
    }
    match s
        .config
        .oauth
        .create_state(&req.client_id, &req.redirect_uri)
    {
        Ok(state) => Json(json!({"state": state})).into_response(),
        Err(e) => {
            mark_persistence_error(&s, &e);
            oauth_error(e)
        }
    }
}
#[derive(Deserialize)]
struct GithubStartQuery {
    state: String,
}

#[derive(Deserialize)]
struct GithubCallbackQuery {
    state: Option<String>,
    code: Option<String>,
}

async fn github_start(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Query(query): Query<GithubStartQuery>,
) -> Response {
    if !persistence_ready(&s) {
        return persistence_unavailable();
    }
    if !limited(&s, &headers, Some(peer.0), "github_start") {
        return rate_error();
    }
    let Some(hosted) = s.config.hosted_auth.as_ref() else {
        return auth_failure();
    };
    cleanup_browser(&s);
    if !s.browser.lock().unwrap().pending.contains_key(&query.state) {
        return auth_failure();
    }
    let github_state = uuid::Uuid::new_v4().to_string();
    let continuation = GithubContinuation {
        mcp_state: query.state,
        expires_at: Instant::now() + Duration::from_secs(600),
    };
    if !persist_session(
        &s,
        &github_state,
        "github",
        None,
        &GithubContinuationRecord {
            mcp_state: continuation.mcp_state.clone(),
        },
        Duration::from_secs(600),
    ) {
        return persistence_unavailable();
    }
    s.browser
        .lock()
        .unwrap()
        .github
        .insert(github_state.clone(), continuation);
    let mut response =
        Redirect::to(hosted.github.authorization_url(&github_state).as_str()).into_response();
    response.headers_mut().append(
        "set-cookie",
        secure_cookie("mcp_github_state", &github_state, 600),
    );
    response
}

async fn github_callback(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Query(query): Query<GithubCallbackQuery>,
) -> Response {
    if !persistence_ready(&s) {
        return persistence_unavailable();
    }
    if !limited(&s, &headers, Some(peer.0), "github_callback") {
        return rate_error();
    }
    let Some(hosted) = s.config.hosted_auth.as_ref() else {
        return auth_failure();
    };
    let Some(state) = query.state.filter(|value| !value.is_empty()) else {
        return auth_failure();
    };
    let Some(code) = query.code.filter(|value| !value.is_empty()) else {
        return auth_failure();
    };
    if cookie(&headers, "mcp_github_state").as_deref() != Some(state.as_str()) {
        return auth_failure();
    }
    cleanup_browser(&s);
    // Consume the durable records first.  Removing the in-memory copy first
    // leaves the durable row available to a concurrent callback, allowing the
    // same GitHub state to resume twice before the cleanup below runs.
    let continuation = consume_json_session::<GithubContinuationRecord>(&s, &state, "github")
        .map(|(value, _)| value)
        .or_else(|| {
            s.browser
                .lock()
                .unwrap()
                .github
                .remove(&state)
                .map(|value| GithubContinuationRecord {
                    mcp_state: value.mcp_state,
                })
        });
    let Some(continuation) = continuation else {
        return auth_failure();
    };
    s.browser.lock().unwrap().github.remove(&state);
    let pending =
        consume_json_session::<PendingAuthorizationRecord>(&s, &continuation.mcp_state, "pending")
            .map(|(value, _)| value)
            .or_else(|| {
                s.browser
                    .lock()
                    .unwrap()
                    .pending
                    .remove(&continuation.mcp_state)
                    .map(|value| PendingAuthorizationRecord {
                        request: value.request,
                        client_state: value.client_state,
                    })
            });
    let Some(pending) = pending else {
        return auth_failure();
    };
    s.browser
        .lock()
        .unwrap()
        .pending
        .remove(&continuation.mcp_state);
    let github_user = match hosted.github.exchange_callback(&code).await {
        Ok(user) => user,
        Err(_) => return auth_failure(),
    };
    let user = match hosted
        .tenant
        .upsert_user(&github_user.github_id, &github_user.login)
    {
        Ok(user) => user,
        Err(_) => return auth_failure(),
    };
    let client_id = pending.request.client_id.clone();
    let client_state = pending.client_state;
    let authorization = match s.config.oauth.authorize_for_user(pending.request, user.id) {
        Ok(mut authorization) => {
            // The client-supplied OAuth state is opaque and must be returned
            // unchanged. The signed service state is kept only server-side to
            // authenticate/resume this transaction.
            authorization.state = client_state;
            authorization
        }
        Err(_) => return auth_failure(),
    };
    if hosted
        .tenant
        .create_grant(user.id, &client_id, &s.config.oauth.resource())
        .is_err()
    {
        return auth_failure();
    }
    let browser_session = uuid::Uuid::new_v4().to_string();
    let csrf = uuid::Uuid::new_v4().to_string();
    if !persist_session(
        &s,
        &browser_session,
        "browser",
        Some(user.id),
        &BrowserSessionRecord { csrf: csrf.clone() },
        s.config.session_ttl,
    ) {
        return auth_failure();
    }
    s.browser.lock().unwrap().sessions.insert(
        browser_session.clone(),
        (user.id, Instant::now() + s.config.session_ttl, csrf.clone()),
    );
    let mut response = authorization_redirect(&s, Ok(authorization));
    response.headers_mut().append(
        "set-cookie",
        secure_cookie(
            "mcp_session",
            &browser_session,
            s.config.session_ttl.as_secs(),
        ),
    );
    response.headers_mut().append(
        "set-cookie",
        readable_cookie("mcp_csrf", &csrf, s.config.session_ttl.as_secs()),
    );
    response
        .headers_mut()
        .append("set-cookie", expired_cookie("mcp_github_state"));
    response
}

#[derive(Deserialize)]
struct SettingsInput {
    base_url: Option<String>,
    access_token: Option<String>,
    profile: Option<String>,
    csrf: Option<String>,
    delete: Option<String>,
}

fn settings_error(code: StatusCode) -> Response {
    (code, Json(json!({"error":"settings request rejected"}))).into_response()
}
fn settings_json(value: Value) -> Response {
    (StatusCode::OK, Json(value)).into_response()
}
fn csrf_valid(headers: &HeaderMap, expected: &str, supplied: Option<&str>) -> bool {
    headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .or(supplied)
        == Some(expected)
}

async fn settings_get(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let Some((user_id, csrf)) = browser_session(&s, &headers) else {
        return settings_error(StatusCode::UNAUTHORIZED);
    };
    let connection = s
        .config
        .hosted_auth
        .as_ref()
        .and_then(|h| h.tenant.load_connection(user_id).ok().flatten());
    let (host, profile) = connection
        .as_ref()
        .map(|c| (c.base_url.host_str().unwrap_or(""), Some(c.profile)))
        .unwrap_or(("", None));
    if headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("application/json"))
    {
        return settings_json(
            json!({"host":host,"configured":!host.is_empty(),"profile":profile.map(profile_name).unwrap_or("read-only"),"last_validated_at":Value::Null}),
        );
    }
    Html(settings_html(Some(host), profile, &csrf)).into_response()
}

async fn settings_save(State(s): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let Some((user_id, expected_csrf)) = browser_session(&s, &headers) else {
        return settings_error(StatusCode::UNAUTHORIZED);
    };
    let input: SettingsInput = if headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("application/x-www-form-urlencoded"))
    {
        serde_urlencoded::from_bytes(&body).unwrap_or(SettingsInput {
            base_url: None,
            access_token: None,
            profile: None,
            csrf: None,
            delete: None,
        })
    } else {
        serde_json::from_slice(&body).unwrap_or(SettingsInput {
            base_url: None,
            access_token: None,
            profile: None,
            csrf: None,
            delete: None,
        })
    };
    if !csrf_valid(&headers, &expected_csrf, input.csrf.as_deref()) {
        return settings_error(StatusCode::FORBIDDEN);
    }
    if input.delete.as_deref() == Some("true") {
        return settings_delete_inner(&s, user_id);
    }
    let (Some(raw_url), Some(token), Some(profile_raw)) =
        (input.base_url, input.access_token, input.profile)
    else {
        return settings_error(StatusCode::BAD_REQUEST);
    };
    let Some(profile) = parse_profile(&profile_raw) else {
        return settings_error(StatusCode::BAD_REQUEST);
    };
    let Ok(base_url) = validate_base_url(&raw_url, s.config.allow_insecure_local_targets) else {
        return settings_error(StatusCode::BAD_REQUEST);
    };
    if token.is_empty() || token.len() > 4096 {
        return settings_error(StatusCode::BAD_REQUEST);
    }
    let Some(hosted) = &s.config.hosted_auth else {
        return settings_error(StatusCode::SERVICE_UNAVAILABLE);
    };
    let secret = secrecy::SecretString::from(token);
    let mut env = std::collections::HashMap::new();
    env.insert(
        "COOLIFY_ACCESS_TOKEN".to_owned(),
        secret.expose_secret().to_owned(),
    );
    let Ok(token_source) = TokenSource::from_env(&env) else {
        return settings_error(StatusCode::BAD_REQUEST);
    };
    let Ok(client) = CoolifyClient::new_hosted_with_local_escape(
        CoolifyConfig {
            base_url: base_url.clone(),
            token_source,
            custom_headers: Default::default(),
            timeout: Duration::from_secs(10),
        },
        s.config.allow_insecure_local_targets,
    ) else {
        return settings_error(StatusCode::BAD_REQUEST);
    };
    let Ok(probe) = client.probe_get("version").await else {
        return settings_error(StatusCode::BAD_GATEWAY);
    };
    if probe.status >= 400 {
        return settings_error(StatusCode::BAD_GATEWAY);
    }
    if hosted
        .tenant
        .save_connection(user_id, &base_url, &secret, profile)
        .is_err()
    {
        return settings_error(StatusCode::BAD_REQUEST);
    }
    let value = json!({"host":base_url.host_str().unwrap_or(""),"configured":true,"profile":profile_name(profile),"last_validated_at":unix_timestamp()});
    if headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("application/json"))
    {
        settings_json(value)
    } else {
        Redirect::to("/settings").into_response()
    }
}

fn settings_delete_inner(s: &AppState, user_id: UserId) -> Response {
    let Some(hosted) = &s.config.hosted_auth else {
        return settings_error(StatusCode::SERVICE_UNAVAILABLE);
    };
    // Revoke bearer grants before deleting the connection. If deletion fails,
    // the connection remains usable only after a fresh authorization, rather
    // than leaving already-issued MCP tokens active.
    if hosted.tenant.revoke_user_grants(user_id).is_err()
        || s.config.oauth.revoke_user(user_id).is_err()
        || hosted.tenant.delete_connection(user_id).is_err()
    {
        return settings_error(StatusCode::INTERNAL_SERVER_ERROR);
    }
    settings_json(json!({"configured":false}))
}
async fn settings_delete(State(s): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let Some((user_id, expected_csrf)) = browser_session(&s, &headers) else {
        return settings_error(StatusCode::UNAUTHORIZED);
    };
    let supplied = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| {
            serde_json::from_slice::<SettingsInput>(&body)
                .ok()
                .and_then(|v| v.csrf)
        });
    if !csrf_valid(&headers, &expected_csrf, supplied.as_deref()) {
        return settings_error(StatusCode::FORBIDDEN);
    }
    settings_delete_inner(&s, user_id)
}
async fn settings_logout(State(s): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let Some((user_id, expected_csrf)) = browser_session(&s, &headers) else {
        return settings_error(StatusCode::UNAUTHORIZED);
    };
    let supplied = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| {
            serde_json::from_slice::<SettingsInput>(&body)
                .ok()
                .and_then(|v| v.csrf)
        });
    if !csrf_valid(&headers, &expected_csrf, supplied.as_deref()) {
        return settings_error(StatusCode::FORBIDDEN);
    }
    if let Some(hosted) = &s.config.hosted_auth {
        // Logout is a security boundary: invalidate bearer grants as well as
        // the browser cookie so another client cannot continue the session.
        if hosted.tenant.revoke_user_grants(user_id).is_err()
            || s.config.oauth.revoke_user(user_id).is_err()
        {
            return settings_error(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    if let Some(session) = cookie(&headers, "mcp_session") {
        s.browser.lock().unwrap().sessions.remove(&session);
        if let Some(store) = tenant_store(&s) {
            let _ = store.delete_session(&session, "browser");
        }
    }
    let mut response = Json(json!({"logged_out":true})).into_response();
    response
        .headers_mut()
        .append("set-cookie", expired_cookie("mcp_session"));
    response
        .headers_mut()
        .append("set-cookie", expired_cookie("mcp_csrf"));
    response
}

#[derive(Serialize, Deserialize)]
struct GithubContinuationRecord {
    mcp_state: String,
}
#[derive(Serialize, Deserialize)]
struct BrowserSessionRecord {
    csrf: String,
}
#[derive(Serialize, Deserialize)]
struct McpSessionRecord {
    principal: String,
}

fn tenant_store(s: &AppState) -> Option<&tenant::TenantStore> {
    s.config.hosted_auth.as_ref().map(|h| h.tenant.as_ref())
}
fn persist_session(
    s: &AppState,
    id: &str,
    kind: &str,
    user: Option<UserId>,
    payload: &impl Serialize,
    ttl: Duration,
) -> bool {
    let Some(store) = tenant_store(s) else {
        return true;
    };
    serde_json::to_vec(payload).ok().is_some_and(|bytes| {
        store
            .put_session(
                id,
                kind,
                user,
                &bytes,
                unix_timestamp() + ttl.as_secs() as i64,
            )
            .is_ok()
    })
}
fn load_json_session<T: for<'de> Deserialize<'de>>(
    s: &AppState,
    id: &str,
    kind: &str,
) -> Option<(T, Option<UserId>)> {
    let record = tenant_store(s)?.load_session(id, kind).ok()??;
    Some((
        serde_json::from_slice(&record.payload).ok()?,
        record.user_id,
    ))
}
fn consume_json_session<T: for<'de> Deserialize<'de>>(
    s: &AppState,
    id: &str,
    kind: &str,
) -> Option<(T, Option<UserId>)> {
    let record = tenant_store(s)?.consume_session(id, kind).ok()??;
    Some((
        serde_json::from_slice(&record.payload).ok()?,
        record.user_id,
    ))
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn cleanup_browser(s: &AppState) {
    let now = Instant::now();
    let mut browser = s.browser.lock().unwrap();
    browser
        .pending
        .retain(|_, pending| pending.expires_at > now);
    browser
        .github
        .retain(|_, continuation| continuation.expires_at > now);
    browser
        .sessions
        .retain(|_, (_, expires_at, _)| *expires_at > now);
}

fn browser_user(s: &AppState, headers: &HeaderMap) -> Option<UserId> {
    let session = cookie(headers, "mcp_session")?;
    cleanup_browser(s);
    if let Some(user_id) = s
        .browser
        .lock()
        .unwrap()
        .sessions
        .get(&session)
        .map(|(user_id, _, _)| *user_id)
    {
        return Some(user_id);
    }
    load_json_session::<BrowserSessionRecord>(s, &session, "browser").and_then(|(_, user)| user)
}

fn browser_session(s: &AppState, headers: &HeaderMap) -> Option<(UserId, String)> {
    let session = cookie(headers, "mcp_session")?;
    cleanup_browser(s);
    if let Some(value) = s
        .browser
        .lock()
        .unwrap()
        .sessions
        .get(&session)
        .map(|(user_id, _, csrf)| (*user_id, csrf.clone()))
    {
        return Some(value);
    }
    load_json_session::<BrowserSessionRecord>(s, &session, "browser")
        .and_then(|(record, user)| Some((user?, record.csrf)))
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value.split(';').find_map(|part| {
                let (key, value) = part.trim().split_once('=')?;
                (key == name).then(|| value.to_owned())
            })
        })
}

fn secure_cookie(name: &str, value: &str, max_age: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}={value}; Max-Age={max_age}; Path=/; Secure; HttpOnly; SameSite=Lax"
    ))
    .expect("cookie values are generated from safe identifiers")
}

fn readable_cookie(name: &str, value: &str, max_age: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}={value}; Max-Age={max_age}; Path=/; Secure; SameSite=Lax"
    ))
    .expect("cookie values are generated from safe identifiers")
}

fn expired_cookie(name: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}=; Max-Age=0; Path=/; Secure; HttpOnly; SameSite=Lax"
    ))
    .expect("cookie name is static")
}

fn auth_failure() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":"authentication_failed","error_description":"authentication failed"})),
    )
        .into_response()
}

#[derive(Deserialize)]
struct AuthorizeQuery {
    client_id: String,
    redirect_uri: String,
    response_type: String,
    resource: String,
    scope: Option<String>,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
}
async fn authorize(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Query(q): Query<AuthorizeQuery>,
) -> Response {
    if !persistence_ready(&s) {
        return persistence_unavailable();
    }
    if !limited(&s, &headers, Some(peer.0), "authorize") {
        return rate_error();
    }
    let req = AuthorizeRequest {
        client_id: q.client_id,
        redirect_uri: q.redirect_uri,
        response_type: q.response_type,
        resource: q.resource,
        scope: q.scope.unwrap_or_default(),
        state: q.state,
        code_challenge: q.code_challenge,
        code_challenge_method: q.code_challenge_method,
    };
    if s.config.hosted_auth.is_some() {
        if let Some(user_id) = browser_user(&s, &headers) {
            return authorization_redirect(&s, s.config.oauth.authorize_for_user(req, user_id));
        }

        let service_state = match s
            .config
            .oauth
            .create_state(&req.client_id, &req.redirect_uri)
        {
            Ok(state) => state,
            Err(error) => {
                mark_persistence_error(&s, &error);
                return oauth_error(error);
            }
        };
        let client_state = req.state.clone();
        let mcp_state = service_state.clone();
        let mut pending_request = req;
        pending_request.state = service_state;
        cleanup_browser(&s);
        let pending_record = PendingAuthorizationRecord {
            request: pending_request.clone(),
            client_state: client_state.clone(),
        };
        let ttl = s.config.session_ttl.min(Duration::from_secs(600));
        if !persist_session(&s, &mcp_state, "pending", None, &pending_record, ttl) {
            return persistence_unavailable();
        }
        s.browser.lock().unwrap().pending.insert(
            mcp_state.clone(),
            PendingAuthorization {
                request: pending_request,
                client_state,
                expires_at: Instant::now() + ttl,
            },
        );
        let mut location = crate::http::public_base(&s.config.public_url);
        location.push_str("/auth/github/start?state=");
        location.push_str(
            &url::form_urlencoded::byte_serialize(mcp_state.as_bytes()).collect::<String>(),
        );
        return Redirect::to(&location).into_response();
    }

    authorization_redirect(&s, s.config.oauth.authorize(req))
}

fn authorization_redirect(
    s: &AppState,
    result: Result<oauth::AuthorizationResponse, OAuthError>,
) -> Response {
    match result {
        Ok(v) => {
            let mut location = v.redirect_uri;
            let separator = if location.contains('?') { '&' } else { '?' };
            location.push(separator);
            location.push_str("code=");
            location.push_str(
                &url::form_urlencoded::byte_serialize(v.code.as_bytes()).collect::<String>(),
            );
            location.push_str("&state=");
            location.push_str(
                &url::form_urlencoded::byte_serialize(v.state.as_bytes()).collect::<String>(),
            );
            Redirect::to(&location).into_response()
        }
        Err(e) => {
            mark_persistence_error(s, &e);
            oauth_error(e)
        }
    }
}
#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: String,
    client_secret: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    resource: Option<String>,
}
async fn token(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    axum::extract::Form(f): axum::extract::Form<TokenForm>,
) -> Response {
    if !persistence_ready(&s) {
        return persistence_unavailable();
    }
    if !limited(&s, &headers, Some(peer.0), "token") {
        return rate_error();
    }
    let req = TokenRequest {
        grant_type: f.grant_type,
        code: f.code.unwrap_or_default(),
        redirect_uri: f.redirect_uri,
        client_id: f.client_id,
        client_secret: f.client_secret,
        code_verifier: f.code_verifier,
        refresh_token: f.refresh_token.clone(),
        resource: f.resource,
    };
    let result = if req.grant_type == "refresh_token" {
        f.refresh_token
            .as_deref()
            .ok_or(OAuthError::InvalidGrant)
            .and_then(|t| s.config.oauth.refresh(t))
    } else {
        s.config.oauth.exchange_code(req)
    };
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            mark_persistence_error(&s, &e);
            oauth_error(e)
        }
    }
}
fn rate_error() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [("retry-after", "60")],
        Json(json!({"error":"rate_limited","error_description":"rate limit exceeded"})),
    )
        .into_response()
}
fn persistence_ready(s: &AppState) -> bool {
    if s.config.hosted_auth.is_some() {
        s.config.persistence_available
            && s.config
                .persistence_health
                .load(std::sync::atomic::Ordering::Acquire)
    } else {
        true
    }
}
fn persistence_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error":"temporarily_unavailable","error_description":"authorization persistence unavailable"})),
    )
        .into_response()
}
fn mark_persistence_error(s: &AppState, error: &OAuthError) {
    if matches!(error, OAuthError::Persistence) {
        s.config
            .persistence_health
            .store(false, std::sync::atomic::Ordering::Release);
    }
}
fn oauth_error(e: OAuthError) -> Response {
    let status = if matches!(e, OAuthError::InvalidClient) {
        StatusCode::UNAUTHORIZED
    } else {
        StatusCode::BAD_REQUEST
    };
    (
        status,
        Json(json!({"error":"invalid_request","error_description":e.to_string()})),
    )
        .into_response()
}
fn authorized(headers: &HeaderMap, s: &AppState) -> Result<String, StatusCode> {
    let value = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if s.config.hosted_auth.is_some() {
        return s
            .config
            .oauth
            .verify_bearer_user(value, &crate::http::mcp_resource_url(&s.config.public_url))
            .map(|user_id| user_id.to_string())
            .map_err(|_| StatusCode::UNAUTHORIZED);
    }
    s.config
        .oauth
        .verify_bearer(value, &crate::http::mcp_resource_url(&s.config.public_url))
        .map_err(|_| StatusCode::UNAUTHORIZED)
}
fn tenant_error(message: &'static str) -> ToolResult {
    ToolResult {
        text: serde_json::to_string(&json!({
            "error": {"code": "MCP_TOOL_ERROR", "message": message, "details": null},
            "is_error": true
        }))
        .unwrap_or_else(|_| "{\"error\":\"tool unavailable\"}".into()),
        is_error: true,
    }
}

/// Resolve a hosted bearer principal into one request-scoped Coolify client.
/// This is deliberately the only HTTP-to-tool construction path: failures are
/// generic and there is no process-global client to fall back to.
fn tenant_tool_context(s: &AppState, principal: &str) -> Result<TenantToolContext, ToolResult> {
    let user_id =
        UserId::parse(principal).ok_or_else(|| tenant_error("tenant authentication required"))?;
    let hosted = s
        .config
        .hosted_auth
        .as_ref()
        .ok_or_else(|| tenant_error("tenant connection unavailable"))?;
    let connection = hosted
        .tenant
        .load_connection(user_id)
        .map_err(|_| tenant_error("tenant connection unavailable"))?
        .ok_or_else(|| tenant_error("tenant connection unavailable"))?;
    let token = connection.token.expose_secret().to_owned();
    let token_source = coolify_api::TokenSource::from_env(&HashMap::from([(
        "COOLIFY_ACCESS_TOKEN".to_owned(),
        token,
    )]))
    .map_err(|_| tenant_error("tenant connection unavailable"))?;
    let client = coolify_api::CoolifyClient::new_hosted_with_local_escape(
        coolify_api::CoolifyConfig {
            base_url: connection.base_url,
            token_source,
            custom_headers: reqwest::header::HeaderMap::new(),
            timeout: Duration::from_secs(45),
        },
        s.config.allow_insecure_local_targets,
    )
    .map_err(|_| tenant_error("tenant connection unavailable"))?;
    Ok(TenantToolContext {
        request: TenantRequestContext {
            user_id,
            profile: connection.profile,
        },
        client: Arc::new(client),
        audit: None,
        instance_registry: None,
        request_metadata: serde_json::Map::new(),
    })
}

fn accepts_json(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',').any(|x| {
                let x = x.trim();
                x == "*/*"
                    || x.starts_with("application/json")
                    || x.starts_with("text/event-stream")
            })
        })
}
fn content_json(headers: &HeaderMap) -> bool {
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|x| x.trim().eq_ignore_ascii_case("application/json"))
        })
}
fn cleanup_sessions(s: &AppState) {
    let now = Instant::now();
    s.sessions
        .lock()
        .unwrap()
        .retain(|_, (_, seen)| now.duration_since(*seen) < s.config.session_ttl);
}
fn session_valid(s: &AppState, id: &str, principal: &str) -> bool {
    if uuid::Uuid::parse_str(id).is_err() {
        return false;
    }
    cleanup_sessions(s);
    let mut sessions = s.sessions.lock().unwrap();
    if let Some((owner, seen)) = sessions.get_mut(id)
        && owner == principal
    {
        *seen = Instant::now();
        true
    } else {
        drop(sessions);
        load_json_session::<McpSessionRecord>(s, id, "mcp")
            .is_some_and(|(record, _)| record.principal == principal)
    }
}
async fn mcp_delete(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let Ok(principal) = authorized(&headers, &s) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(id) = headers.get("mcp-session-id").and_then(|v| v.to_str().ok()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !session_valid(&s, id, &principal) {
        return StatusCode::NOT_FOUND.into_response();
    }
    s.sessions.lock().unwrap().remove(id);
    if let Some(store) = tenant_store(&s) {
        let _ = store.delete_session(id, "mcp");
    }
    StatusCode::NO_CONTENT.into_response()
}
async fn mcp_get(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let Ok(principal) = authorized(&headers, &s) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"invalid bearer token"})),
        )
            .into_response();
    };
    if !accepts_json(&headers) {
        return (
            StatusCode::NOT_ACCEPTABLE,
            Json(json!({"error":"acceptable Accept header required"})),
        )
            .into_response();
    }
    let Some(id) = headers.get("mcp-session-id").and_then(|v| v.to_str().ok()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"Mcp-Session-Id required"})),
        )
            .into_response();
    };
    if !session_valid(&s, id, &principal) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"unknown MCP session"})),
        )
            .into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}
async fn mcp(State(s): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let started = Instant::now();
    let parsed_body = serde_json::from_slice::<Value>(&body).ok();
    let audit_name = parsed_body
        .as_ref()
        .and_then(|v| v.pointer("/params/name"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let audit_args = parsed_body
        .as_ref()
        .and_then(|v| v.pointer("/params/arguments"));
    let client = match authorized(&headers, &s) {
        Ok(client) => Some(client),
        Err(_) => {
            audit_event(
                &s,
                None,
                audit_name,
                audit_args,
                "rejected",
                StatusCode::UNAUTHORIZED,
                started,
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error":"invalid bearer token"})),
            )
                .into_response();
        }
    };
    if !content_json(&headers) || !accepts_json(&headers) {
        audit_event(
            &s,
            client.as_deref(),
            audit_name,
            audit_args,
            "rejected",
            StatusCode::NOT_ACCEPTABLE,
            started,
        );
        return (
            StatusCode::NOT_ACCEPTABLE,
            Json(json!({"error":"Content-Type application/json and acceptable Accept required"})),
        )
            .into_response();
    }
    let request: Value = match parsed_body {
        Some(v) => v,
        None => {
            audit_event(
                &s,
                client.as_deref(),
                audit_name,
                None,
                "rejected",
                StatusCode::BAD_REQUEST,
                started,
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                ),
            )
                .into_response();
        }
    };
    if request.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    let provided = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if let Some(id) = &provided
        && !session_valid(&s, id, client.as_deref().unwrap_or(""))
    {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"unknown MCP session"})),
        )
            .into_response();
    }
    let id = request["id"].clone();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let response = match method {
        "initialize" => {
            json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"coolify-mcp","version":env!("CARGO_PKG_VERSION")}}})
        }
        "tools/list" => {
            if s.config.hosted_auth.is_some() {
                match client.as_deref() {
                    Some(principal) => match tenant_tool_context(&s, principal) {
                        Ok(context) => json!({
                            "jsonrpc":"2.0",
                            "id":id,
                            "result":{"tools":s.app.tools_for_user(context.request)}
                        }),
                        Err(error) => json!({
                            "jsonrpc":"2.0",
                            "id":id,
                            "result":{"content":[{"type":"text","text":error.text}],"isError":true}
                        }),
                    },
                    None => {
                        let error = tenant_error("tenant authentication required");
                        json!({
                            "jsonrpc":"2.0",
                            "id":id,
                            "result":{"content":[{"type":"text","text":error.text}],"isError":true}
                        })
                    }
                }
            } else {
                json!({"jsonrpc":"2.0","id":id,"result":{"tools":s.app.tools()}})
            }
        }
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let r = if s.config.hosted_auth.is_some() {
                match client.as_deref() {
                    Some(principal) => match tenant_tool_context(&s, principal) {
                        Ok(context) => s.app.call_for_user(context, name, args).await,
                        Err(error) => error,
                    },
                    None => tenant_error("tenant authentication required"),
                }
            } else {
                s.app.call(name, args).await
            };
            audit_event(
                &s,
                client.as_deref(),
                name,
                request.pointer("/params/arguments"),
                if r.is_error { "error" } else { "ok" },
                StatusCode::OK,
                started,
            );
            json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":r.text}],"isError":r.is_error}})
        }
        _ => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
    };
    let session = if let Some(id) = provided {
        id
    } else {
        cleanup_sessions(&s);
        if let Some(store) = tenant_store(&s) {
            match store.count_sessions("mcp") {
                Ok(count) if count >= s.config.max_sessions => {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(json!({"error":"MCP session capacity reached"})),
                    )
                        .into_response();
                }
                Err(_) => return persistence_unavailable(),
                _ => {}
            }
        }
        let mut sessions = s.sessions.lock().unwrap();
        if sessions.len() >= s.config.max_sessions {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":"MCP session capacity reached"})),
            )
                .into_response();
        }
        let id = uuid::Uuid::new_v4().to_string();
        let principal = client.as_deref().unwrap_or_default().to_owned();
        sessions.insert(id.clone(), (principal.clone(), Instant::now()));
        if s.config.hosted_auth.is_some()
            && !persist_session(
                &s,
                &id,
                "mcp",
                UserId::parse(&principal),
                &McpSessionRecord { principal },
                s.config.session_ttl,
            )
        {
            return persistence_unavailable();
        }
        id
    };
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    response
        .headers_mut()
        .insert("mcp-session-id", session.parse().unwrap());
    response
}
