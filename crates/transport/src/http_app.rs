use crate::http::HttpConfig;
use axum::extract::DefaultBodyLimit;
use axum::{
    Router,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Redirect, Response},
    routing::{get, post},
};
use mcp_tools::{McpApplication, ToolResult};
use oauth::{AuthorizeRequest, OAuthError, RegistrationRequest, TokenRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    app: Arc<dyn McpApplication>,
    config: HttpConfig,
}
struct EmptyApp;
impl McpApplication for EmptyApp {
    fn tools(&self) -> Vec<mcp_tools::ToolSpec> {
        Vec::new()
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
    router_with_app(config, EmptyApp)
}
pub fn router_with_app<A: McpApplication + 'static>(config: HttpConfig, app: A) -> Router {
    let state = AppState {
        app: Arc::new(app),
        config,
    };
    Router::new()
        .route("/healthz", get(health))
        .route("/.well-known/oauth-authorization-server", get(discovery))
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route("/oauth/register", post(register))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/token", post(token))
        .route("/mcp", post(mcp).get(mcp_get))
        .layer(DefaultBodyLimit::max(state.config.max_body_bytes))
        .with_state(state)
}
async fn health() -> impl IntoResponse {
    Json(json!({"status":"ok"}))
}
async fn discovery(State(s): State<AppState>) -> impl IntoResponse {
    Json(
        json!({"issuer":s.config.public_url.to_string(),"authorization_endpoint":format!("{}/oauth/authorize",s.config.public_url),"token_endpoint":format!("{}/oauth/token",s.config.public_url),"registration_endpoint":format!("{}/oauth/register",s.config.public_url),"response_types_supported":["code"],"code_challenge_methods_supported":["S256"]}),
    )
}
async fn protected_resource(State(s): State<AppState>) -> impl IntoResponse {
    Json(
        json!({"resource":format!("{}/mcp",s.config.public_url),"authorization_servers":[s.config.public_url.to_string()]}),
    )
}
async fn register(State(s): State<AppState>, Json(req): Json<RegistrationRequest>) -> Response {
    match s.config.oauth.register(req) {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => oauth_error(e),
    }
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
async fn authorize(State(s): State<AppState>, Query(q): Query<AuthorizeQuery>) -> Response {
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
    match s.config.oauth.authorize(req) {
        Ok(v) => Redirect::to(&format!(
            "{}?code={}&state={}",
            v.redirect_uri, v.code, v.state
        ))
        .into_response(),
        Err(e) => oauth_error(e),
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
    axum::extract::Form(f): axum::extract::Form<TokenForm>,
) -> Response {
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
        Err(e) => oauth_error(e),
    }
}
fn oauth_error(e: OAuthError) -> Response {
    let status = match e {
        OAuthError::InvalidClient => StatusCode::UNAUTHORIZED,
        _ => StatusCode::BAD_REQUEST,
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
    s.config
        .oauth
        .verify_bearer(value, &format!("{}/mcp", s.config.public_url))
        .map_err(|_| StatusCode::UNAUTHORIZED)
}
async fn mcp_get() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({"error":"POST required"})),
    )
        .into_response()
}
async fn mcp(State(s): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if authorized(&headers, &s).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"invalid bearer token"})),
        )
            .into_response();
    }
    let request: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (
            StatusCode::BAD_REQUEST,
            Json(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
            ),
        )
            .into_response(),
    };
    if request.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let response = match method {
        "initialize" => {
            json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"coolify-mcp","version":env!("CARGO_PKG_VERSION")}}})
        }
        "tools/list" => json!({"jsonrpc":"2.0","id":id,"result":{"tools":s.app.tools()}}),
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let r = s.app.call(name, args).await;
            json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":r.text}],"isError":r.is_error}})
        }
        _ => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
    };
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    response.headers_mut().insert(
        "mcp-session-id",
        format!("{}", uuid::Uuid::new_v4()).parse().unwrap(),
    );
    response
}
