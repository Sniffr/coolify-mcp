#!/bin/sh
set -eu
REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'
}
FIXTURE_PORT=${FIXTURE_PORT:-$(free_port)}
GITHUB_PORT=${GITHUB_PORT:-$(free_port)}
MCP_PORT=${MCP_PORT:-$(free_port)}
FIXTURE="${REPO_ROOT}/target/debug/fake-coolify"
GITHUB="${REPO_ROOT}/target/debug/fake-github"
MCP="${REPO_ROOT}/target/debug/sniffr-coolify-mcp"
CLIENT="${REPO_ROOT}/target/debug/multitenant-http-interop"
OAUTH_DIR=$(mktemp -d)
fixture_pid=""; github_pid=""; mcp_pid=""
cleanup() { kill ${fixture_pid:-} ${github_pid:-} ${mcp_pid:-} >/dev/null 2>&1 || true; }
trap 'cleanup; rm -rf "$OAUTH_DIR"' EXIT INT TERM
(cd "$REPO_ROOT" && cargo build --quiet --bin fake-coolify --bin fake-github --bin sniffr-coolify-mcp --bin multitenant-http-interop)
"$FIXTURE" "127.0.0.1:$FIXTURE_PORT" >"$OAUTH_DIR/coolify.stdout" 2>"$OAUTH_DIR/coolify.stderr" & fixture_pid=$!
"$GITHUB" "127.0.0.1:$GITHUB_PORT" >"$OAUTH_DIR/github.stdout" 2>"$OAUTH_DIR/github.stderr" & github_pid=$!
for endpoint in "http://127.0.0.1:$FIXTURE_PORT/__fixture__/stats" "http://127.0.0.1:$GITHUB_PORT/__fixture__/stats"; do
  ready=0
  for _ in $(seq 1 50); do
    if curl --silent --output /dev/null --max-time 2 "$endpoint"; then ready=1; break; fi
    sleep 0.1
  done
  test "$ready" = 1 || { echo 'fixture failed to become ready' >&2; exit 1; }
done
MCP_TRANSPORT=http MCP_PUBLIC_URL="http://127.0.0.1:$MCP_PORT" MCP_ALLOW_INSECURE_HTTP=true MCP_HOSTED_INSECURE_LOCAL_TARGETS=true MCP_PORT="$MCP_PORT" \
  MCP_DATABASE_PATH="$OAUTH_DIR/tenant.sqlite" MCP_CONNECTION_ENCRYPTION_KEY='fixture-encryption-key-material-only-for-acceptance' \
  GITHUB_CLIENT_ID='fixture-github-client-id' GITHUB_CLIENT_SECRET='fixture-github-client-secret' \
  GITHUB_CALLBACK_URL="http://127.0.0.1:$MCP_PORT/auth/github/callback" \
  MCP_GITHUB_TOKEN_ENDPOINT="http://127.0.0.1:$GITHUB_PORT/login/oauth/access_token" MCP_GITHUB_USER_ENDPOINT="http://127.0.0.1:$GITHUB_PORT/user" \
  MCP_OAUTH_STATE_FILE="$OAUTH_DIR/oauth-state.json" MCP_AUDIT_LOG="$OAUTH_DIR/audit.jsonl" \
  "$MCP" >"$OAUTH_DIR/mcp.stdout" 2>"$OAUTH_DIR/mcp.stderr" & mcp_pid=$!
mcp_ready=0
for _ in $(seq 1 50); do
  if curl --silent --output /dev/null --max-time 2 "http://127.0.0.1:$MCP_PORT/healthz"; then
    mcp_ready=1
    break
  fi
  sleep 0.2
done
test "$mcp_ready" = 1 || { echo 'MCP server failed to become ready' >&2; exit 1; }
"$CLIENT" "http://127.0.0.1:$MCP_PORT" "http://127.0.0.1:$GITHUB_PORT" "http://127.0.0.1:$FIXTURE_PORT" >"$OAUTH_DIR/client.stdout" 2>"$OAUTH_DIR/client.stderr"
fixture_stats=$(curl --silent --fail --max-time 5 "http://127.0.0.1:$FIXTURE_PORT/__fixture__/stats")
github_stats=$(curl --silent --fail --max-time 5 "http://127.0.0.1:$GITHUB_PORT/__fixture__/stats")
python3 - "$fixture_stats" "$github_stats" "$OAUTH_DIR" <<'EOF'
import json, pathlib, sys
coolify, github = json.loads(sys.argv[1]), json.loads(sys.argv[2])
assert coolify["unauthorized"] == 0, coolify
assert coolify["unknown_paths"] == 0, coolify
assert coolify["requests"] == 6, coolify
assert coolify["calls"] == [
  "GET /api/v1/version user_a", "GET /api/v1/version user_a",
  "GET /api/v1/applications?page=1&per_page=50 user_a",
  "GET /api/v1/version user_b", "GET /api/v1/version user_b",
  "GET /api/v1/applications?page=1&per_page=50 user_b",
], coolify
assert github["unauthorized"] == 0 and github["invalid_codes"] == 0, github
assert github["token_exchanges"] == 2 and github["user_fetches"] == 2, github
root = pathlib.Path(sys.argv[3])
all_text = "\n".join(p.read_text(errors="replace") for p in root.glob("**/*") if p.is_file())
for secret in ("coolify-fixture-token-a", "coolify-fixture-token-b", "github-fixture-token", "fixture-github-client-secret"):
    assert secret not in all_text, secret
assert "fixture-app-a" not in all_text and "fixture-app-b" not in all_text
EOF
printf '%s\n' 'two-user hosted acceptance passed'
