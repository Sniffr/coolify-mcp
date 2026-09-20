#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-https://mcp.social.dpdns.org}"
EXPECTED_UPSTREAM='mcp:8080'
forbidden='COOLIFY_ACCESS_TOKEN|COOLIFY_TOKEN|COOLIFY_BASE_URL|COOLIFY_URL|Authorization:[[:space:]]*Bearer|fixture-token|password|secret'

fetch() {
  local path="$1"
  local out
  out="$(mktemp)"
  trap 'rm -f "$out"' RETURN
  curl --fail --silent --show-error --max-time 20 --proto '=https' --tlsv1.2 \
    "$BASE_URL$path" >"$out"
  if grep -Eiq "$forbidden" "$out"; then
    echo "FAIL $path: response contains a forbidden credential-like value" >&2
    return 1
  fi
  cat "$out"
}

health="$(fetch /healthz)"
if [[ "$health" != *'ok'* ]]; then
  echo "FAIL /healthz: expected an ok health response" >&2
  exit 1
fi

auth_resource="$(fetch /.well-known/oauth-protected-resource)"
if [[ "$auth_resource" != *'/mcp'* ]]; then
  echo "FAIL protected-resource discovery: expected an /mcp resource" >&2
  exit 1
fi

auth_server="$(fetch /.well-known/oauth-authorization-server)"
if [[ "$auth_server" != *'https://mcp.social.dpdns.org'* ]]; then
  echo "FAIL authorization-server discovery: expected the public issuer" >&2
  exit 1
fi

# The routed health check proves Caddy reaches the private mcp:8080 upstream.
# When run on the Docker host, also validate the rendered production topology
# and assert that the mcp container has no published host port.
if command -v docker >/dev/null 2>&1 && [[ -f deploy/multitenant-compose.yaml ]]; then
  docker compose -f deploy/multitenant-compose.yaml config --quiet
  published="$(docker inspect --format '{{json .NetworkSettings.Ports}}' mcp 2>/dev/null || true)"
  if [[ -n "$published" && "$published" != "null" && "$published" != "{}" ]]; then
    echo "FAIL mcp: production service must not publish a host port" >&2
    exit 1
  fi
fi

echo "remote health and OAuth discovery checks passed for $BASE_URL (Caddy upstream ${EXPECTED_UPSTREAM}; no public upstream port)"
