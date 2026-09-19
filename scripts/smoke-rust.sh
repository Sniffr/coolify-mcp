#!/bin/sh
set -eu
BASE_URL=${1:-http://127.0.0.1:8080}
case "$BASE_URL" in *[!A-Za-z0-9:/._-]*) echo 'invalid URL' >&2; exit 2;; esac
health=$(curl --fail --silent --show-error --max-time 10 "$BASE_URL/healthz")
echo "$health" | grep -q '"status":"ok"'
# The protected endpoint must not be usable without an OAuth bearer; do not send or print credentials.
status=$(curl --silent --output /dev/null --write-out '%{http_code}' --max-time 10 \
  -H 'content-type: application/json' -H 'accept: application/json' \
  -X POST "$BASE_URL/mcp" || true)
test "$status" = 401
echo 'Rust container smoke test passed'
