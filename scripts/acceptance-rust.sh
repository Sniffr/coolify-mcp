#!/bin/sh
set -eu
REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'
}
FIXTURE_PORT=${FIXTURE_PORT:-$(free_port)}
MCP_PORT=${MCP_PORT:-$(free_port)}
FIXTURE="$REPO_ROOT/target/debug/fake-coolify"
MCP="$REPO_ROOT/target/debug/sniffr-coolify-mcp"
OAUTH_DIR=$(mktemp -d)
fixture_pid=""
mcp_pid=""
cleanup() { kill ${fixture_pid:-} ${mcp_pid:-} >/dev/null 2>&1 || true; }
trap 'cleanup; rm -rf "$OAUTH_DIR"' EXIT INT TERM
(cd "$REPO_ROOT" && cargo build --quiet --bin fake-coolify --bin sniffr-coolify-mcp --bin http-oauth-interop --bin stdio-interop)
"$FIXTURE" "127.0.0.1:$FIXTURE_PORT" >/dev/null 2>&1 & fixture_pid=$!
trap 'cleanup; rm -rf "$OAUTH_DIR"' EXIT INT TERM
fixture_ready=0
for _ in 1 2 3 4 5 6 7 8 9 10; do
  if curl --silent --output /dev/null --max-time 2 "http://127.0.0.1:$FIXTURE_PORT/__fixture__/stats"; then
    fixture_ready=1
    break
  fi
  sleep 0.2
done
test "$fixture_ready" = 1 || { echo 'fixture failed to become ready' >&2; exit 1; }
MCP_TRANSPORT=http MCP_PUBLIC_URL="http://127.0.0.1:$MCP_PORT" MCP_ALLOW_INSECURE_HTTP=true MCP_PORT="$MCP_PORT" \
  MCP_CAPABILITY_PROFILE=operations \
  COOLIFY_BASE_URL="http://127.0.0.1:$FIXTURE_PORT" COOLIFY_ACCESS_TOKEN=fixture-token \
  MCP_OAUTH_STATE_FILE="$OAUTH_DIR/oauth-state.json" \
  "$MCP" >/dev/null 2>&1 & mcp_pid=$!
mcp_ready=0
for _ in $(seq 1 50); do
  if curl --silent --output /dev/null --max-time 2 "http://127.0.0.1:$MCP_PORT/healthz"; then
    mcp_ready=1
    break
  fi
  sleep 0.2
done
test "$mcp_ready" = 1 || { echo 'MCP server failed to become ready' >&2; exit 1; }
"$REPO_ROOT/target/debug/http-oauth-interop" "http://127.0.0.1:$MCP_PORT"
COOLIFY_BASE_URL="http://127.0.0.1:$FIXTURE_PORT" "$REPO_ROOT/target/debug/stdio-interop" "$MCP"
fixture_stats=$(curl --silent --fail --max-time 5 "http://127.0.0.1:$FIXTURE_PORT/__fixture__/stats")
echo "$fixture_stats" | grep -q '"unauthorized":0'
requests=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["requests"])' <<EOF
$fixture_stats
EOF
)
unauthorized=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["unauthorized"])' <<EOF
$fixture_stats
EOF
)
test "$unauthorized" = 0
# 3 HTTP calls + 6 stdio calls (2 framing modes x 3 tools). A tight bound
# proves the safe calls occurred, no unauthorized requests happened, and no
# unsafe compatibility fallback retries inflated the traffic.
test "$requests" -ge 9
test "$requests" -le 12
echo 'Rust local acceptance passed'
