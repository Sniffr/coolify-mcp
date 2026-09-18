# Coolify API — LLM-Optimized Reference

> Compressed from the official Coolify API documentation. Verify endpoint schemas against the live docs for the installed Coolify version: <https://coolify.io/docs/api/overview>. Documentation reviewed: overview, making requests, authorization, permissions, IP allowlist, rate limits, errors, and the endpoint index. Generated 2026-09-18.

## TL;DR: easiest LLM integration

Use a tiny deterministic REST tool wrapper rather than giving an LLM unrestricted shell access:

1. Store `COOLIFY_URL` and `COOLIFY_TOKEN` outside the prompt, ideally in a secret manager.
2. Give the model a tool such as `coolify_request(method, path, query, body)` that allows only approved paths/methods.
3. The wrapper prepends `/api/v1`, sends `Authorization: Bearer $COOLIFY_TOKEN` and `Accept: application/json`, adds JSON content type for bodies, validates inputs, and returns status/headers/body.
4. Start with a token having only `read`; use `deploy`, `write`, or `read:sensitive` only when necessary. Avoid `root`.
5. Make the model discover UUIDs first, then call mutations. Require confirmation for destructive or production-affecting operations.
6. Treat HTTP status as authoritative, honor `Retry-After`, and never retry unsafe writes automatically.

Coolify is a REST API. No special SDK is required; `curl`, Python `requests/httpx`, or JavaScript `fetch` are sufficient.

## Use this tool from Claude Code and OpenHands

The included `coolify_mcp_server.py` is a dependency-free MCP server over stdio. It is shared by any MCP-capable client, including Claude Code and OpenHands. It reads `COOLIFY_URL` and `COOLIFY_TOKEN` from the server process environment; the token is never placed in the tool definition or prompt.

### Configure credentials safely

Set these in the environment of the client process or its secret manager:

```bash
export COOLIFY_URL='https://your-coolify.example'
export COOLIFY_TOKEN='<complete-token-from-coolify>'
```

Do not commit these values. Do not pass the token as a tool argument. Use a dedicated least-privilege token and a separate token per team/integration.

### Claude Code configuration

Add an MCP server entry to Claude Code’s MCP configuration (project-local `.mcp.json` or your user-level MCP config, depending on your desired scope):

```json
{
  "mcpServers": {
    "coolify": {
      "command": "python3",
      "args": ["/absolute/path/to/coolify_mcp_server.py"],
      "env": {
        "COOLIFY_URL": "${COOLIFY_URL}",
        "COOLIFY_TOKEN": "${COOLIFY_TOKEN}"
      }
    }
  }
}
```

If your Claude Code version does not expand `${...}` in MCP `env`, launch Claude Code from a shell where the variables are exported, or use its supported secret/environment configuration. Confirm the server appears as `coolify_request` and `coolify_health`; then ask Claude to list resources before allowing mutations.

### OpenHands configuration

Register the same command as a local MCP server in the OpenHands MCP/tool settings available in your deployment. Use:

- command: `python3`
- arguments: `/absolute/path/to/coolify_mcp_server.py`
- environment: `COOLIFY_URL` and `COOLIFY_TOKEN`

OpenHands deployments can also expose the server through the MCP configuration used by the agent runtime. Keep the environment variables in the agent-server/container secret store, not in repository files. The server is stdio-based, so it must run in the same environment as the client or be launched through a secure process bridge.

### Tool safety model

The server exposes two tools:

- `coolify_health`: public reachability check.
- `coolify_request`: generic documented REST call with method/path/query/body.

The generic tool intentionally does not auto-execute policy decisions: the host LLM/client must request confirmation for writes, deploys, deletes, restarts, stops, cancellations, production changes, and sensitive reads. For a stricter production setup, replace the generic tool with an allowlisted operation catalog generated from the OpenAPI section below, or add a proxy that rejects unapproved method/path pairs.

### Smoke test without exposing a token

```bash
python3 coolify_mcp_server.py
```

The process waits for MCP JSON-RPC messages on stdin; normally Claude Code or OpenHands launches it. Test with the client’s MCP inspector rather than piping credentials into shell history. A health call should work without `COOLIFY_TOKEN`; all protected calls require it.

### Important limitation

The endpoint catalog below covers every operation in the current official `openapi.json` snapshot (276 operations / 193 paths), but it is generated metadata rather than a replacement for each endpoint’s full schema. For deployment automation, use the exact request body schema and required permission shown by the live endpoint/OpenAPI document, and regenerate this section when upgrading Coolify.


## Connection and authentication

| Deployment | Base URL |
|---|---|
| Coolify Cloud | `https://app.coolify.io/api/v1` |
| Self-hosted | `https://<your-coolify-domain>/api/v1` |

Health checks are public and do not need a token: `GET /api/health` or `GET /api/v1/health`; they return plain-text `OK`.

Protected requests use the complete token as a Bearer credential. A token includes an ID, `|`, and secret; do not strip any part.

```bash
export COOLIFY_URL="https://your-coolify.example"
export COOLIFY_TOKEN="<complete-api-token>"

curl --fail-with-body --request GET \
  --header "Authorization: Bearer $COOLIFY_TOKEN" \
  --header "Accept: application/json" \
  "$COOLIFY_URL/api/v1/teams/current"
```

For `POST`/`PATCH`/other JSON bodies, add `Content-Type: application/json` and send the endpoint’s documented fields/types:

```bash
curl --fail-with-body --request PATCH \
  --header "Authorization: Bearer $COOLIFY_TOKEN" \
  --header "Accept: application/json" \
  --header "Content-Type: application/json" \
  --data '{"name":"production-platform"}' \
  "$COOLIFY_URL/api/v1/projects/<project-uuid>"
```

Endpoint paths are combined with the base URL. Example: `PATCH /projects/{uuid}` becomes `.../api/v1/projects/<uuid>`. Query parameters remain in the URL, e.g. `POST /deploy?uuid=<application-uuid>&force=true`.

## Token design and least privilege

Tokens belong to the user and **active team at creation time**. Switching teams later does not change the token. Use a separate token per integration and team; rotate/revoke tokens and never put them in source code, URLs, prompts, or logs.

| Permission | Use |
|---|---|
| `read` | List and inspect resources; sensitive values stay redacted. |
| `deploy` | Trigger deployments, restarts, stops, cancellations, and deploy webhooks. |
| `write` | Create, update, and delete resources; owner must remain team admin/owner. |
| `read:sensitive` | Secrets, logs, passwords, private keys, environment values, and Compose content where supported; also grants `read`; owner must be admin/owner. |
| `root` | Bypasses endpoint `read`/`write`/`deploy` checks, but remains team-bound and does not bypass instance API access/IP allowlists; reserve for exceptional cases. |

The token owner must still belong to the team. Privileged permissions (`write`, `deploy`, `read:sensitive`, `root`) require team administrator/owner where documented. Token permissions cannot be edited after creation: create a replacement. A password change revokes that user’s issued tokens.

## Self-hosted prerequisites and networking

An administrator must enable authenticated API access under **Settings → Configuration → Advanced → API Settings**. Optional **Allowed IPs for API Access** accepts comma-separated exact IPv4/IPv6 addresses and CIDR networks. A blocked client receives `403`, even with a valid token. Empty or `0.0.0.0` allows any source; prefer restrictive production allowlists.

If the caller is a container on Coolify’s Docker network, it can use `http://coolify:8080/api/v1`. The allowlist sees the container’s address, potentially IPv6; inspect both subnets:

```bash
docker network inspect coolify --format '{{range .IPAM.Config}}{{println .Subnet}}{{end}}'
```

Only allow trusted containers/networks. Cloud customers do not use these self-hosted instance controls.

## LLM tool wrapper pattern

Expose a constrained function like:

```json
{
  "name": "coolify_request",
  "description": "Call an approved Coolify API operation",
  "parameters": {
    "type": "object",
    "required": ["method", "path"],
    "properties": {
      "method": {"enum": ["GET", "POST", "PATCH", "PUT", "DELETE"]},
      "path": {"type": "string", "description": "Path under /api/v1; never a full URL"},
      "query": {"type": "object"},
      "body": {"type": ["object", "null"]}
    }
  }
}
```

Wrapper rules:

- Allowlist endpoint/method pairs; reject arbitrary hostnames, schemes, headers, and path traversal.
- Validate UUIDs, query values, and body fields against the endpoint schema.
- Redact tokens and sensitive response fields in model-visible logs unless explicitly required.
- Return a stable envelope: `{status, headers: {retry_after, rate_remaining}, body}`.
- Require explicit user confirmation before `DELETE`, `write`, `deploy`, production changes, or reading sensitive data.
- Keep an audit log outside the model context.
- Add idempotency/state checks in the wrapper where possible; Coolify endpoint behavior is operation-specific.
- Prefer structured endpoint definitions/OpenAPI-derived schemas over asking the model to invent paths.

Minimal Python shape:

```python
import os
import requests

BASE = os.environ["COOLIFY_URL"].rstrip("/") + "/api/v1"
TOKEN = os.environ["COOLIFY_TOKEN"]

def coolify_request(method, path, query=None, body=None):
    if not path.startswith("/") or ".." in path or path.startswith("//"):
        raise ValueError("invalid API path")
    r = requests.request(
        method, BASE + path, params=query, json=body,
        headers={"Authorization": f"Bearer {TOKEN}", "Accept": "application/json",
                 **({"Content-Type": "application/json"} if body is not None else {})},
        timeout=30,
    )
    return {"status": r.status_code, "headers": {
        "retry_after": r.headers.get("Retry-After"),
        "rate_remaining": r.headers.get("X-RateLimit-Remaining"),
    }, "body": r.json() if r.content else None}
```

## Recommended agent workflow

1. **Inspect:** call a read-only team/projects/resources endpoint to discover accessible scope and UUIDs.
2. **Plan:** map the user’s request to one documented endpoint and required permission; state the intended mutation.
3. **Confirm:** ask before deployment, restart, stop, cancellation, deletion, configuration change, or sensitive read.
4. **Execute:** use exact HTTP method/path/body from the endpoint page.
5. **Verify:** inspect response status/body, then query resource/deployment status if the operation is asynchronous.
6. **Report:** summarize what changed, UUIDs, status, and any follow-up action; never expose the token.

The API index groups endpoints for Applications, Cloud init scripts/tokens, Databases, Deployments, Destinations, DigitalOcean, GitHub Apps, GitLab Apps, Hetzner, Notifications, Private Keys, Projects, Resources, S3 Storages, Scheduled Tasks, Servers, Service Applications/Databases, Services, Shared Environment Variables, System, Tags, Teams, and Vultr. Each endpoint page is the source of truth for its method, path, required permission, request body, parameters, and response schema.

## Responses, errors, and retries

Most responses are JSON. Errors usually contain `message`; validation errors may add `errors` keyed by field. Decide success from HTTP status, not a human-readable message. A strange IP-allowlist response may say `"success": true` while the HTTP status is `403`—the status wins.

| Status | Meaning / agent action |
|---|---|
| `200` | Success; read result. |
| `201` | Created; store returned identifier and verify state if needed. |
| `400` | Invalid request/token; check schema, parameters, complete token. |
| `401` | Missing/invalid/expired/revoked token or team membership; re-authenticate. |
| `403` | Permission, team role, API access, or source-IP failure. |
| `404` | Wrong route/resource UUID or resource unavailable to token’s team. |
| `409` | State conflict (for example, existing domain); inspect and resolve before retry. |
| `422` | Field validation failure; fix `errors`, then retry. |
| `429` | Rate limit, deployment queue, or upstream provider busy; follow retry policy. |

On successful calls, Coolify may omit resources from another team or redact passwords, keys, environment values, logs, or Compose content. Use a separate token for another team and `read:sensitive` only when needed.

## Rate limits

Defaults: general API `200 requests/minute`; health endpoint `1,000/minute`. Coolify Cloud uses these defaults; self-hosted can change general limits via `API_RATE_LIMIT`. Buckets identify authenticated users when possible, otherwise source IP.

Read these headers:

- `X-RateLimit-Limit`: window maximum
- `X-RateLimit-Remaining`: remaining after current request
- `Retry-After`: seconds to wait on rate-limited responses
- `X-RateLimit-Reset`: Unix timestamp for another attempt

On `429`: stop the affected operation, wait `Retry-After`, then use exponential backoff plus jitter if needed. Do not parallel-retry the same request. Avoid automatic retries for unsafe writes unless the operation is demonstrably idempotent and duplicate-safe. Cache and batch reads before increasing server limits.

## Endpoint discovery and documentation

- Overview: <https://coolify.io/docs/api/overview>
- Request construction: <https://coolify.io/docs/api/making-requests>
- Authorization: <https://coolify.io/docs/api/authorization>
- Permissions: <https://coolify.io/docs/api/permissions>
- IP allowlist: <https://coolify.io/docs/api/ip-allowlist>
- Rate limits: <https://coolify.io/docs/api/rate-limits>
- Errors: <https://coolify.io/docs/api/errors>
- API token creation/security: <https://coolify.io/docs/core/security/credentials/api-tokens>
- Endpoint pages: linked from the **Endpoints** section in the API sidebar.

### Practical system prompt for an API-capable LLM

```text
You operate Coolify only through the coolify_request tool. Use documented endpoint paths and schemas; never invent an endpoint. Discover UUIDs with read-only calls before mutations. Keep credentials secret. Explain the intended write/deploy/delete action and request confirmation first. Use the minimum token permission. Treat HTTP status as authoritative, return structured errors, honor Retry-After, and do not retry unsafe writes. After mutations, verify resource/deployment state and report the exact result.
```

## Complete endpoint catalog (generated from official OpenAPI)

> The current official OpenAPI document contains 193 paths and 276 operations. This catalog records every operation, method, path, parameters, and JSON body schema name. The endpoint page/OpenAPI schema remains authoritative for enums, types, defaults, and response schemas. Source: <https://raw.githubusercontent.com/coollabsio/coolify/main/openapi.json>.

### Applications

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/applications` | `list-applications` — List | `tag` | — |
| `POST` | `/applications/dockerfile` | `create-dockerfile-application` — Create (Dockerfile without git) | — | `project_uuid`, `server_uuid`, `environment_name`, `environment_uuid`, `dockerfile`, `build_pack`, `ports_exposes`, `destination_uuid`, `name`, `description`, `domains`, `noindex_domains`, `docker_registry_image_name`, `docker_registry_image_tag`, `ports_mappings`, `base_directory`, `health_check_enabled`, `health_check_path`, `health_check_port`, `health_check_host`, `health_check_method`, `health_check_return_code`, `health_check_scheme`, `health_check_response_text`, `health_check_interval`, `health_check_timeout`, `health_check_retries`, `health_check_start_period`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `custom_labels`, `custom_docker_run_options`, `post_deployment_command`, `post_deployment_command_container`, `pre_deployment_command`, `pre_deployment_command_container`, `manual_webhook_secret_github`, `manual_webhook_secret_gitlab`, `manual_webhook_secret_bitbucket`, `manual_webhook_secret_gitea`, `redirect`, `instant_deploy`, `is_force_https_enabled`, `is_preview_deployments_enabled`, `use_build_server`, `use_build_secrets`, `is_git_submodules_enabled`, `is_git_lfs_enabled`, `is_git_shallow_clone_enabled`, `disable_build_cache`, `inject_build_args_to_dockerfile`, `include_source_commit_in_build`, `is_env_sorting_enabled`, `is_pr_deployments_public_enabled`, `stop_grace_period`, `docker_images_to_keep`, `is_gzip_enabled`, `is_stripprefix_enabled`, `is_raw_compose_deployment_enabled`, `is_log_drain_enabled`, `is_gpu_enabled`, `gpu_driver`, `gpu_count`, `gpu_device_ids`, `gpu_options`, `is_consistent_container_name_enabled`, `custom_internal_name`, `preview_url_template`, `max_restart_count`, `is_http_basic_auth_enabled`, `http_basic_auth_username`, `http_basic_auth_password`, `connect_to_docker_network`, `force_domain_override`, `autogenerate_domain`, `is_container_label_escape_enabled`, `tags` |
| `POST` | `/applications/dockerimage` | `create-dockerimage-application` — Create (Docker Image without git) | — | `project_uuid`, `server_uuid`, `environment_name`, `environment_uuid`, `docker_registry_image_name`, `docker_registry_image_tag`, `ports_exposes`, `destination_uuid`, `name`, `description`, `domains`, `noindex_domains`, `ports_mappings`, `health_check_enabled`, `health_check_path`, `health_check_port`, `health_check_host`, `health_check_method`, `health_check_return_code`, `health_check_scheme`, `health_check_response_text`, `health_check_interval`, `health_check_timeout`, `health_check_retries`, `health_check_start_period`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `custom_labels`, `custom_docker_run_options`, `post_deployment_command`, `post_deployment_command_container`, `pre_deployment_command`, `pre_deployment_command_container`, `manual_webhook_secret_github`, `manual_webhook_secret_gitlab`, `manual_webhook_secret_bitbucket`, `manual_webhook_secret_gitea`, `redirect`, `instant_deploy`, `is_force_https_enabled`, `is_preview_deployments_enabled`, `use_build_server`, `use_build_secrets`, `is_git_submodules_enabled`, `is_git_lfs_enabled`, `is_git_shallow_clone_enabled`, `disable_build_cache`, `inject_build_args_to_dockerfile`, `include_source_commit_in_build`, `is_env_sorting_enabled`, `is_pr_deployments_public_enabled`, `stop_grace_period`, `docker_images_to_keep`, `is_gzip_enabled`, `is_stripprefix_enabled`, `is_raw_compose_deployment_enabled`, `is_log_drain_enabled`, `is_gpu_enabled`, `gpu_driver`, `gpu_count`, `gpu_device_ids`, `gpu_options`, `is_consistent_container_name_enabled`, `custom_internal_name`, `preview_url_template`, `max_restart_count`, `is_http_basic_auth_enabled`, `http_basic_auth_username`, `http_basic_auth_password`, `connect_to_docker_network`, `force_domain_override`, `autogenerate_domain`, `is_container_label_escape_enabled`, `tags` |
| `POST` | `/applications/private-deploy-key` | `create-private-deploy-key-application` — Create (Private - Deploy Key) | — | `project_uuid`, `server_uuid`, `environment_name`, `environment_uuid`, `private_key_uuid`, `git_repository`, `git_branch`, `ports_exposes`, `destination_uuid`, `build_pack`, `name`, `description`, `domains`, `noindex_domains`, `git_commit_sha`, `docker_registry_image_name`, `docker_registry_image_tag`, `is_static`, `is_spa`, `is_auto_deploy_enabled`, `is_force_https_enabled`, `is_preview_deployments_enabled`, `static_image`, `install_command`, `build_command`, `start_command`, `ports_mappings`, `base_directory`, `publish_directory`, `health_check_enabled`, `health_check_path`, `health_check_port`, `health_check_host`, `health_check_method`, `health_check_return_code`, `health_check_scheme`, `health_check_response_text`, `health_check_interval`, `health_check_timeout`, `health_check_retries`, `health_check_start_period`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `custom_labels`, `custom_docker_run_options`, `post_deployment_command`, `post_deployment_command_container`, `pre_deployment_command`, `pre_deployment_command_container`, `manual_webhook_secret_github`, `manual_webhook_secret_gitlab`, `manual_webhook_secret_bitbucket`, `manual_webhook_secret_gitea`, `redirect`, `instant_deploy`, `dockerfile`, `dockerfile_location`, `docker_compose_location`, `docker_compose_custom_start_command`, `docker_compose_custom_build_command`, `docker_compose_domains`, `watch_paths`, `use_build_server`, `use_build_secrets`, `is_git_submodules_enabled`, `is_git_lfs_enabled`, `is_git_shallow_clone_enabled`, `disable_build_cache`, `inject_build_args_to_dockerfile`, `include_source_commit_in_build`, `is_env_sorting_enabled`, `is_pr_deployments_public_enabled`, `stop_grace_period`, `docker_images_to_keep`, `is_gzip_enabled`, `is_stripprefix_enabled`, `is_raw_compose_deployment_enabled`, `is_log_drain_enabled`, `is_gpu_enabled`, `gpu_driver`, `gpu_count`, `gpu_device_ids`, `gpu_options`, `is_consistent_container_name_enabled`, `custom_internal_name`, `preview_url_template`, `max_restart_count`, `is_http_basic_auth_enabled`, `http_basic_auth_username`, `http_basic_auth_password`, `connect_to_docker_network`, `force_domain_override`, `autogenerate_domain`, `is_container_label_escape_enabled`, `tags`, `is_preserve_repository_enabled` |
| `POST` | `/applications/private-github-app` | `create-private-github-app-application` — Create (Private - GH App) | — | `project_uuid`, `server_uuid`, `environment_name`, `environment_uuid`, `github_app_uuid`, `git_repository`, `git_branch`, `ports_exposes`, `destination_uuid`, `build_pack`, `name`, `description`, `domains`, `noindex_domains`, `git_commit_sha`, `docker_registry_image_name`, `docker_registry_image_tag`, `is_static`, `is_spa`, `is_auto_deploy_enabled`, `is_force_https_enabled`, `is_preview_deployments_enabled`, `static_image`, `install_command`, `build_command`, `start_command`, `ports_mappings`, `base_directory`, `publish_directory`, `health_check_enabled`, `health_check_path`, `health_check_port`, `health_check_host`, `health_check_method`, `health_check_return_code`, `health_check_scheme`, `health_check_response_text`, `health_check_interval`, `health_check_timeout`, `health_check_retries`, `health_check_start_period`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `custom_labels`, `custom_docker_run_options`, `post_deployment_command`, `post_deployment_command_container`, `pre_deployment_command`, `pre_deployment_command_container`, `manual_webhook_secret_github`, `manual_webhook_secret_gitlab`, `manual_webhook_secret_bitbucket`, `manual_webhook_secret_gitea`, `redirect`, `instant_deploy`, `dockerfile`, `dockerfile_location`, `docker_compose_location`, `docker_compose_custom_start_command`, `docker_compose_custom_build_command`, `docker_compose_domains`, `watch_paths`, `use_build_server`, `use_build_secrets`, `is_git_submodules_enabled`, `is_git_lfs_enabled`, `is_git_shallow_clone_enabled`, `disable_build_cache`, `inject_build_args_to_dockerfile`, `include_source_commit_in_build`, `is_env_sorting_enabled`, `is_pr_deployments_public_enabled`, `stop_grace_period`, `docker_images_to_keep`, `is_gzip_enabled`, `is_stripprefix_enabled`, `is_raw_compose_deployment_enabled`, `is_log_drain_enabled`, `is_gpu_enabled`, `gpu_driver`, `gpu_count`, `gpu_device_ids`, `gpu_options`, `is_consistent_container_name_enabled`, `custom_internal_name`, `preview_url_template`, `max_restart_count`, `is_http_basic_auth_enabled`, `http_basic_auth_username`, `http_basic_auth_password`, `connect_to_docker_network`, `force_domain_override`, `autogenerate_domain`, `is_container_label_escape_enabled`, `tags`, `is_preserve_repository_enabled` |
| `POST` | `/applications/public` | `create-public-application` — Create (Public) | — | `project_uuid`, `server_uuid`, `environment_name`, `environment_uuid`, `git_repository`, `git_branch`, `build_pack`, `ports_exposes`, `destination_uuid`, `name`, `description`, `domains`, `noindex_domains`, `git_commit_sha`, `docker_registry_image_name`, `docker_registry_image_tag`, `is_static`, `is_spa`, `is_auto_deploy_enabled`, `is_force_https_enabled`, `is_preview_deployments_enabled`, `static_image`, `install_command`, `build_command`, `start_command`, `ports_mappings`, `base_directory`, `publish_directory`, `health_check_enabled`, `health_check_path`, `health_check_port`, `health_check_host`, `health_check_method`, `health_check_return_code`, `health_check_scheme`, `health_check_response_text`, `health_check_interval`, `health_check_timeout`, `health_check_retries`, `health_check_start_period`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `custom_labels`, `custom_docker_run_options`, `post_deployment_command`, `post_deployment_command_container`, `pre_deployment_command`, `pre_deployment_command_container`, `manual_webhook_secret_github`, `manual_webhook_secret_gitlab`, `manual_webhook_secret_bitbucket`, `manual_webhook_secret_gitea`, `redirect`, `instant_deploy`, `dockerfile`, `dockerfile_location`, `docker_compose_location`, `docker_compose_custom_start_command`, `docker_compose_custom_build_command`, `docker_compose_domains`, `watch_paths`, `use_build_server`, `use_build_secrets`, `is_git_submodules_enabled`, `is_git_lfs_enabled`, `is_git_shallow_clone_enabled`, `disable_build_cache`, `inject_build_args_to_dockerfile`, `include_source_commit_in_build`, `is_env_sorting_enabled`, `is_pr_deployments_public_enabled`, `stop_grace_period`, `docker_images_to_keep`, `is_gzip_enabled`, `is_stripprefix_enabled`, `is_raw_compose_deployment_enabled`, `is_log_drain_enabled`, `is_gpu_enabled`, `gpu_driver`, `gpu_count`, `gpu_device_ids`, `gpu_options`, `is_consistent_container_name_enabled`, `custom_internal_name`, `preview_url_template`, `max_restart_count`, `is_http_basic_auth_enabled`, `http_basic_auth_username`, `http_basic_auth_password`, `connect_to_docker_network`, `force_domain_override`, `autogenerate_domain`, `is_container_label_escape_enabled`, `tags`, `is_preserve_repository_enabled` |
| `DELETE` | `/applications/{uuid}` | `delete-application-by-uuid` — Delete | `uuid (required)`, `delete_configurations`, `delete_volumes`, `docker_cleanup`, `delete_connected_networks` | — |
| `GET` | `/applications/{uuid}` | `get-application-by-uuid` — Get | `uuid (required)` | — |
| `PATCH` | `/applications/{uuid}` | `update-application-by-uuid` — Update | `uuid (required)` | `project_uuid`, `server_uuid`, `environment_name`, `github_app_uuid`, `git_repository`, `git_branch`, `ports_exposes`, `destination_uuid`, `build_pack`, `name`, `description`, `domains`, `noindex_domains`, `git_commit_sha`, `docker_registry_image_name`, `docker_registry_image_tag`, `is_static`, `is_spa`, `is_auto_deploy_enabled`, `is_force_https_enabled`, `is_preview_deployments_enabled`, `install_command`, `build_command`, `start_command`, `ports_mappings`, `base_directory`, `publish_directory`, `health_check_enabled`, `health_check_path`, `health_check_port`, `health_check_host`, `health_check_method`, `health_check_return_code`, `health_check_scheme`, `health_check_response_text`, `health_check_interval`, `health_check_timeout`, `health_check_retries`, `health_check_start_period`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `custom_labels`, `custom_docker_run_options`, `post_deployment_command`, `post_deployment_command_container`, `pre_deployment_command`, `pre_deployment_command_container`, `manual_webhook_secret_github`, `manual_webhook_secret_gitlab`, `manual_webhook_secret_bitbucket`, `manual_webhook_secret_gitea`, `redirect`, `instant_deploy`, `dockerfile`, `dockerfile_location`, `docker_compose_location`, `docker_compose_custom_start_command`, `docker_compose_custom_build_command`, `docker_compose_domains`, `watch_paths`, `use_build_server`, `use_build_secrets`, `is_git_submodules_enabled`, `is_git_lfs_enabled`, `is_git_shallow_clone_enabled`, `disable_build_cache`, `inject_build_args_to_dockerfile`, `include_source_commit_in_build`, `is_env_sorting_enabled`, `is_pr_deployments_public_enabled`, `stop_grace_period`, `docker_images_to_keep`, `is_gzip_enabled`, `is_stripprefix_enabled`, `is_raw_compose_deployment_enabled`, `is_log_drain_enabled`, `is_gpu_enabled`, `gpu_driver`, `gpu_count`, `gpu_device_ids`, `gpu_options`, `is_consistent_container_name_enabled`, `custom_internal_name`, `preview_url_template`, `max_restart_count`, `connect_to_docker_network`, `force_domain_override`, `is_container_label_escape_enabled`, `is_preserve_repository_enabled` |
| `POST` | `/applications/{uuid}/clone` | `clone-application-by-uuid` — Clone | `uuid (required)` | `destination_uuid`, `name`, `clone_volumes` |
| `GET` | `/applications/{uuid}/destinations` | `list-application-destinations` — List Destinations | `uuid (required)` | — |
| `POST` | `/applications/{uuid}/destinations` | `add-application-destination` — Add Destination | `uuid (required)` | `destination_uuid` |
| `DELETE` | `/applications/{uuid}/destinations/{destination_uuid}` | `remove-application-destination` — Remove Destination | `uuid (required)`, `destination_uuid (required)` | — |
| `GET` | `/applications/{uuid}/envs` | `list-envs-by-application-uuid` — List Envs | `uuid (required)` | — |
| `PATCH` | `/applications/{uuid}/envs` | `update-env-by-application-uuid` — Update Env | `uuid (required)` | `key`, `value`, `is_preview`, `is_literal`, `is_multiline`, `is_shown_once` |
| `POST` | `/applications/{uuid}/envs` | `create-env-by-application-uuid` — Create Env | `uuid (required)` | `key`, `value`, `is_preview`, `is_literal`, `is_multiline`, `is_shown_once` |
| `PATCH` | `/applications/{uuid}/envs/bulk` | `update-envs-by-application-uuid` — Update Envs (Bulk) | `uuid (required)` | `data` |
| `DELETE` | `/applications/{uuid}/envs/{env_uuid}` | `delete-env-by-application-uuid` — Delete Env | `uuid (required)`, `env_uuid (required)` | — |
| `GET` | `/applications/{uuid}/logs` | `get-application-logs-by-uuid` — Get application logs. | `uuid (required)`, `lines`, `show_timestamps` | — |
| `POST` | `/applications/{uuid}/migrate` | `migrate-application-by-uuid` — Migrate to Server | `uuid (required)` | `destination_uuid`, `migrate_volumes` |
| `POST` | `/applications/{uuid}/move` | `move-application-by-uuid` — Move | `uuid (required)` | `environment_uuid` |
| `DELETE` | `/applications/{uuid}/previews/{pull_request_id}` | `delete-preview-deployment-by-pull-request-id` — Delete Preview Deployment | `uuid (required)`, `pull_request_id (required)` | — |
| `GET` | `/applications/{uuid}/previews/{pull_request_id}/logs` | `get-preview-application-logs-by-pull-request-id` — Get preview application logs. | `uuid (required)`, `pull_request_id (required)`, `lines`, `show_timestamps` | — |
| `POST` | `/applications/{uuid}/restart` | `restart-application-by-uuid` — Restart | `uuid (required)` | — |
| `POST` | `/applications/{uuid}/rollback` | `rollback-application-by-uuid` — Rollback | `uuid (required)` | `commit` |
| `GET` | `/applications/{uuid}/rollback-images` | `list-application-rollback-images` — List Rollback Images | `uuid (required)` | — |
| `POST` | `/applications/{uuid}/start` | `start-application-by-uuid` — Start | `uuid (required)`, `force`, `instant_deploy` | — |
| `POST` | `/applications/{uuid}/stop` | `stop-application-by-uuid` — Stop | `uuid (required)`, `docker_cleanup` | — |
| `GET` | `/applications/{uuid}/storages` | `list-storages-by-application-uuid` — List Storages | `uuid (required)` | — |
| `PATCH` | `/applications/{uuid}/storages` | `update-storage-by-application-uuid` — Update Storage | `uuid (required)` | `uuid`, `id`, `type`, `is_preview_suffix_enabled`, `name`, `mount_path`, `content` |
| `POST` | `/applications/{uuid}/storages` | `create-storage-by-application-uuid` — Create Storage | `uuid (required)` | `type`, `name`, `mount_path`, `content`, `is_directory`, `fs_path` |
| `DELETE` | `/applications/{uuid}/storages/{storage_uuid}` | `delete-storage-by-application-uuid` — Delete Storage | `uuid (required)`, `storage_uuid (required)` | — |
| `DELETE` | `/applications/{uuid}/storages/{storage_uuid}/backups` | `delete-application-storage-backup-schedule` — Delete application storage backup schedule | `uuid (required)`, `storage_uuid (required)` | — |
| `PUT` | `/applications/{uuid}/storages/{storage_uuid}/backups` | `set-application-storage-backup-schedule` — Set application storage backup schedule | `uuid (required)`, `storage_uuid (required)` | `VolumeBackupScheduleRequest` |
| `POST` | `/applications/{uuid}/storages/{storage_uuid}/backups/run` | `run-application-storage-backup` — Run application storage backup | `uuid (required)`, `storage_uuid (required)` | — |
| `GET` | `/applications/{uuid}/tags` | `list-tags-by-application-uuid` — List Tags | `uuid (required)` | — |
| `POST` | `/applications/{uuid}/tags` | `create-tag-by-application-uuid` — Create Tag | `uuid (required)` | `tag_name`, `tag_names` |
| `DELETE` | `/applications/{uuid}/tags/{tag_uuid}` | `delete-tag-by-application-uuid` — Delete Tag | `uuid (required)`, `tag_uuid (required)` | — |
### Cloud Tokens

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/cloud-tokens` | `list-cloud-tokens` — List Cloud Provider Tokens | — | — |
| `POST` | `/cloud-tokens` | `create-cloud-token` — Create Cloud Provider Token | — | `provider`, `token`, `name` |
| `DELETE` | `/cloud-tokens/{uuid}` | `delete-cloud-token-by-uuid` — Delete Cloud Provider Token | `uuid (required)` | — |
| `GET` | `/cloud-tokens/{uuid}` | `get-cloud-token-by-uuid` — Get Cloud Provider Token | `uuid (required)` | — |
| `PATCH` | `/cloud-tokens/{uuid}` | `update-cloud-token-by-uuid` — Update Cloud Provider Token | `uuid (required)` | `name` |
| `POST` | `/cloud-tokens/{uuid}/validate` | `validate-cloud-token-by-uuid` — Validate Cloud Provider Token | `uuid (required)` | — |
### Cloud-init Scripts

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/cloud-init-scripts` | `list-cloud-init-scripts` — List Cloud-init Scripts | — | — |
| `POST` | `/cloud-init-scripts` | `create-cloud-init-script` — Create Cloud-init Script | — | `name`, `script` |
| `DELETE` | `/cloud-init-scripts/{uuid}` | `delete-cloud-init-script-by-uuid` — Delete Cloud-init Script | `uuid (required)` | — |
| `GET` | `/cloud-init-scripts/{uuid}` | `get-cloud-init-script-by-uuid` — Get Cloud-init Script | `uuid (required)` | — |
| `PATCH` | `/cloud-init-scripts/{uuid}` | `update-cloud-init-script-by-uuid` — Update Cloud-init Script | `uuid (required)` | `name`, `script` |
### Databases

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/databases` | `list-databases` — List | — | — |
| `POST` | `/databases/clickhouse` | `create-database-clickhouse` — Create (Clickhouse) | — | `server_uuid`, `project_uuid`, `environment_name`, `environment_uuid`, `destination_uuid`, `clickhouse_admin_user`, `clickhouse_admin_password`, `name`, `description`, `image`, `is_public`, `public_port`, `public_port_timeout`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `instant_deploy`, `tags` |
| `POST` | `/databases/dragonfly` | `create-database-dragonfly` — Create (DragonFly) | — | `server_uuid`, `project_uuid`, `environment_name`, `environment_uuid`, `destination_uuid`, `dragonfly_password`, `name`, `description`, `image`, `is_public`, `public_port`, `public_port_timeout`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `instant_deploy`, `tags` |
| `POST` | `/databases/keydb` | `create-database-keydb` — Create (KeyDB) | — | `server_uuid`, `project_uuid`, `environment_name`, `environment_uuid`, `destination_uuid`, `keydb_password`, `keydb_conf`, `name`, `description`, `image`, `is_public`, `public_port`, `public_port_timeout`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `instant_deploy`, `tags` |
| `POST` | `/databases/mariadb` | `create-database-mariadb` — Create (MariaDB) | — | `server_uuid`, `project_uuid`, `environment_name`, `environment_uuid`, `destination_uuid`, `mariadb_conf`, `mariadb_root_password`, `mariadb_user`, `mariadb_password`, `mariadb_database`, `name`, `description`, `image`, `is_public`, `public_port`, `public_port_timeout`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `instant_deploy`, `tags` |
| `POST` | `/databases/mongodb` | `create-database-mongodb` — Create (MongoDB) | — | `server_uuid`, `project_uuid`, `environment_name`, `environment_uuid`, `destination_uuid`, `mongo_conf`, `mongo_initdb_root_username`, `name`, `description`, `image`, `is_public`, `public_port`, `public_port_timeout`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `instant_deploy`, `tags` |
| `POST` | `/databases/mysql` | `create-database-mysql` — Create (MySQL) | — | `server_uuid`, `project_uuid`, `environment_name`, `environment_uuid`, `destination_uuid`, `mysql_root_password`, `mysql_password`, `mysql_user`, `mysql_database`, `mysql_conf`, `name`, `description`, `image`, `is_public`, `public_port`, `public_port_timeout`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `instant_deploy`, `tags` |
| `POST` | `/databases/postgresql` | `create-database-postgresql` — Create (PostgreSQL) | — | `server_uuid`, `project_uuid`, `environment_name`, `environment_uuid`, `postgres_user`, `postgres_password`, `postgres_db`, `postgres_initdb_args`, `postgres_host_auth_method`, `postgres_conf`, `destination_uuid`, `name`, `description`, `image`, `is_public`, `public_port`, `public_port_timeout`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `instant_deploy`, `tags` |
| `POST` | `/databases/redis` | `create-database-redis` — Create (Redis) | — | `server_uuid`, `project_uuid`, `environment_name`, `environment_uuid`, `destination_uuid`, `redis_password`, `redis_conf`, `name`, `description`, `image`, `is_public`, `public_port`, `public_port_timeout`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `instant_deploy`, `tags` |
| `DELETE` | `/databases/{uuid}` | `delete-database-by-uuid` — Delete | `uuid (required)`, `delete_configurations`, `delete_volumes`, `docker_cleanup`, `delete_connected_networks` | — |
| `GET` | `/databases/{uuid}` | `get-database-by-uuid` — Get | `uuid (required)` | — |
| `PATCH` | `/databases/{uuid}` | `update-database-by-uuid` — Update | `uuid (required)` | `name`, `description`, `image`, `is_public`, `public_port`, `public_port_timeout`, `limits_memory`, `limits_memory_swap`, `limits_memory_swappiness`, `limits_memory_reservation`, `limits_cpus`, `limits_cpuset`, `limits_cpu_shares`, `postgres_user`, `postgres_password`, `postgres_db`, `postgres_initdb_args`, `postgres_host_auth_method`, `postgres_conf`, `clickhouse_admin_user`, `clickhouse_admin_password`, `dragonfly_password`, `redis_password`, `redis_conf`, `keydb_password`, `keydb_conf`, `mariadb_conf`, `mariadb_root_password`, `mariadb_user`, `mariadb_password`, `mariadb_database`, `mongo_conf`, `mongo_initdb_root_username`, `mongo_initdb_root_password`, `mongo_initdb_database`, `mysql_root_password`, `mysql_password`, `mysql_user`, `mysql_database`, `mysql_conf`, `health_check_enabled`, `health_check_interval`, `health_check_timeout`, `health_check_retries`, `health_check_start_period` |
| `GET` | `/databases/{uuid}/backups` | `get-database-backups-by-uuid` — Get | `uuid (required)` | — |
| `POST` | `/databases/{uuid}/backups` | `create-database-backup` — Create Backup | `uuid (required)` | `frequency`, `enabled`, `save_s3`, `s3_storage_uuid`, `databases_to_backup`, `dump_all`, `backup_now`, `database_backup_retention_amount_locally`, `database_backup_retention_days_locally`, `database_backup_retention_max_storage_locally`, `database_backup_retention_amount_s3`, `database_backup_retention_days_s3`, `database_backup_retention_max_storage_s3`, `timeout` |
| `DELETE` | `/databases/{uuid}/backups/{scheduled_backup_uuid}` | `delete-backup-configuration-by-uuid` — Delete backup configuration | `uuid (required)`, `scheduled_backup_uuid (required)`, `delete_s3` | — |
| `PATCH` | `/databases/{uuid}/backups/{scheduled_backup_uuid}` | `update-database-backup` — Update | `uuid (required)`, `scheduled_backup_uuid (required)` | `save_s3`, `s3_storage_uuid`, `backup_now`, `enabled`, `databases_to_backup`, `dump_all`, `frequency`, `database_backup_retention_amount_locally`, `database_backup_retention_days_locally`, `database_backup_retention_max_storage_locally`, `database_backup_retention_amount_s3`, `database_backup_retention_days_s3`, `database_backup_retention_max_storage_s3`, `timeout` |
| `GET` | `/databases/{uuid}/backups/{scheduled_backup_uuid}/executions` | `list-backup-executions` — List backup executions | `uuid (required)`, `scheduled_backup_uuid (required)` | — |
| `DELETE` | `/databases/{uuid}/backups/{scheduled_backup_uuid}/executions/{execution_uuid}` | `delete-backup-execution-by-uuid` — Delete backup execution | `uuid (required)`, `scheduled_backup_uuid (required)`, `execution_uuid (required)`, `delete_s3` | — |
| `POST` | `/databases/{uuid}/clone` | `clone-database-by-uuid` — Clone | `uuid (required)` | `destination_uuid`, `name`, `clone_volumes` |
| `GET` | `/databases/{uuid}/envs` | `list-envs-by-database-uuid` — List Envs | `uuid (required)` | — |
| `PATCH` | `/databases/{uuid}/envs` | `update-env-by-database-uuid` — Update Env | `uuid (required)` | `key`, `value`, `is_literal`, `is_multiline`, `is_shown_once` |
| `POST` | `/databases/{uuid}/envs` | `create-env-by-database-uuid` — Create Env | `uuid (required)` | `key`, `value`, `is_literal`, `is_multiline`, `is_shown_once` |
| `PATCH` | `/databases/{uuid}/envs/bulk` | `update-envs-by-database-uuid` — Update Envs (Bulk) | `uuid (required)` | `data` |
| `DELETE` | `/databases/{uuid}/envs/{env_uuid}` | `delete-env-by-database-uuid` — Delete Env | `uuid (required)`, `env_uuid (required)` | — |
| `GET` | `/databases/{uuid}/logs` | `get-database-logs-by-uuid` — Get database logs. | `uuid (required)`, `lines`, `show_timestamps` | — |
| `POST` | `/databases/{uuid}/migrate` | `migrate-database-by-uuid` — Migrate to Server | `uuid (required)` | `destination_uuid`, `migrate_volumes` |
| `POST` | `/databases/{uuid}/move` | `move-database-by-uuid` — Move | `uuid (required)` | `environment_uuid` |
| `POST` | `/databases/{uuid}/restart` | `restart-database-by-uuid` — Restart | `uuid (required)` | — |
| `POST` | `/databases/{uuid}/start` | `start-database-by-uuid` — Start | `uuid (required)` | — |
| `POST` | `/databases/{uuid}/stop` | `stop-database-by-uuid` — Stop | `uuid (required)`, `docker_cleanup` | — |
| `GET` | `/databases/{uuid}/storages` | `list-storages-by-database-uuid` — List Storages | `uuid (required)` | — |
| `PATCH` | `/databases/{uuid}/storages` | `update-storage-by-database-uuid` — Update Storage | `uuid (required)` | `uuid`, `id`, `type`, `is_preview_suffix_enabled`, `name`, `mount_path`, `content` |
| `POST` | `/databases/{uuid}/storages` | `create-storage-by-database-uuid` — Create Storage | `uuid (required)` | `type`, `name`, `mount_path`, `content`, `is_directory`, `fs_path` |
| `DELETE` | `/databases/{uuid}/storages/{storage_uuid}` | `delete-storage-by-database-uuid` — Delete Storage | `uuid (required)`, `storage_uuid (required)` | — |
| `DELETE` | `/databases/{uuid}/storages/{storage_uuid}/backups` | `delete-database-storage-backup-schedule` — Delete database storage backup schedule | `uuid (required)`, `storage_uuid (required)` | — |
| `PUT` | `/databases/{uuid}/storages/{storage_uuid}/backups` | `set-database-storage-backup-schedule` — Set database storage backup schedule | `uuid (required)`, `storage_uuid (required)` | `VolumeBackupScheduleRequest` |
| `POST` | `/databases/{uuid}/storages/{storage_uuid}/backups/run` | `run-database-storage-backup` — Run database storage backup | `uuid (required)`, `storage_uuid (required)` | — |
| `GET` | `/databases/{uuid}/tags` | `list-tags-by-database-uuid` — List Tags | `uuid (required)` | — |
| `POST` | `/databases/{uuid}/tags` | `create-tag-by-database-uuid` — Create Tag | `uuid (required)` | `tag_name`, `tag_names` |
| `DELETE` | `/databases/{uuid}/tags/{tag_uuid}` | `delete-tag-by-database-uuid` — Delete Tag | `uuid (required)`, `tag_uuid (required)` | — |
### Deployments

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `POST` | `/deploy` | `deploy-by-tag-or-uuid` — Deploy | `tag`, `uuid`, `force`, `pr`, `pull_request_id`, `docker_tag` | — |
| `GET` | `/deployments` | `list-deployments` — List | — | — |
| `GET` | `/deployments/applications/{uuid}` | `list-deployments-by-app-uuid` — List application deployments | `uuid (required)`, `skip`, `take` | — |
| `GET` | `/deployments/{uuid}` | `get-deployment-by-uuid` — Get | `uuid (required)` | — |
| `POST` | `/deployments/{uuid}/cancel` | `cancel-deployment-by-uuid` — Cancel | `uuid (required)` | — |
### Destinations

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/destinations` | `list-destinations` — List destinations | — | — |
| `DELETE` | `/destinations/{uuid}` | `delete-destination-by-uuid` — Delete destination | `uuid (required)` | — |
| `GET` | `/destinations/{uuid}` | `get-destination-by-uuid` — Get destination | `uuid (required)` | — |
| `PATCH` | `/destinations/{uuid}` | `update-destination-by-uuid` — Update destination | `uuid (required)` | `name` |
| `GET` | `/servers/{server_uuid}/destinations` | `list-server-destinations` — List destinations by server | `server_uuid (required)` | — |
| `POST` | `/servers/{server_uuid}/destinations` | `create-server-destination` — Create destination | `server_uuid (required)` | `name`, `network`, `type` |
### DigitalOcean

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/digitalocean/images` | `get-digitalocean-images` — Get DigitalOcean images | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `GET` | `/digitalocean/regions` | `get-digitalocean-regions` — Get DigitalOcean regions | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `GET` | `/digitalocean/sizes` | `get-digitalocean-sizes` — Get DigitalOcean sizes | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `GET` | `/digitalocean/ssh-keys` | `get-digitalocean-ssh-keys` — Get DigitalOcean SSH keys | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `POST` | `/servers/digitalocean` | `create-digitalocean-server` — Create a server on DigitalOcean | — | — |
### GitHub Apps

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/github-apps` | `list-github-apps` — List | — | — |
| `POST` | `/github-apps` | `create-github-app` — Create GitHub App | — | `name`, `organization`, `api_url`, `html_url`, `custom_user`, `custom_port`, `app_id`, `installation_id`, `client_id`, `client_secret`, `webhook_secret`, `private_key_uuid`, `is_system_wide` |
| `DELETE` | `/github-apps/{github_app_id}` | `deleteGithubApp` — Delete GitHub App | `github_app_id (required)` | — |
| `PATCH` | `/github-apps/{github_app_id}` | `updateGithubApp` — Update GitHub App | `github_app_id (required)` | `name`, `organization`, `api_url`, `html_url`, `custom_user`, `custom_port`, `app_id`, `installation_id`, `client_id`, `client_secret`, `webhook_secret`, `private_key_uuid`, `is_system_wide` |
| `GET` | `/github-apps/{github_app_id}/repositories` | `load-repositories` — Load Repositories for a GitHub App | `github_app_id (required)` | — |
| `GET` | `/github-apps/{github_app_id}/repositories/{owner}/{repo}/branches` | `load-branches` — Load Branches for a GitHub Repository | `github_app_id (required)`, `owner (required)`, `repo (required)` | — |
### GitLab Apps

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/gitlab-apps` | `list-gitlab-apps` — List | — | — |
| `POST` | `/gitlab-apps` | `create-gitlab-app` — Create GitLab App | — | `name`, `html_url`, `api_url`, `custom_user`, `custom_port`, `group_name`, `client_id`, `client_secret`, `webhook_token`, `redirect_uri`, `is_system_wide` |
| `DELETE` | `/gitlab-apps/{gitlab_app_id}` | `deleteGitlabApp` — Delete GitLab App | `gitlab_app_id (required)` | — |
| `PATCH` | `/gitlab-apps/{gitlab_app_id}` | `updateGitlabApp` — Update GitLab App | `gitlab_app_id (required)` | `name`, `html_url`, `api_url`, `custom_user`, `custom_port`, `group_name`, `client_id`, `client_secret`, `webhook_token`, `redirect_uri`, `is_system_wide` |
### Hetzner

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/hetzner/firewalls` | `get-hetzner-firewalls` — Get Hetzner Firewalls | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `GET` | `/hetzner/images` | `get-hetzner-images` — Get Hetzner Images | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `GET` | `/hetzner/locations` | `get-hetzner-locations` — Get Hetzner Locations | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `GET` | `/hetzner/networks` | `get-hetzner-networks` — Get Hetzner Networks | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `GET` | `/hetzner/server-types` | `get-hetzner-server-types` — Get Hetzner Server Types | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `GET` | `/hetzner/ssh-keys` | `get-hetzner-ssh-keys` — Get Hetzner SSH Keys | `cloud_provider_token_uuid`, `cloud_provider_token_id` | — |
| `POST` | `/servers/hetzner` | `create-hetzner-server` — Create Hetzner Server | — | `cloud_provider_token_uuid`, `cloud_provider_token_id`, `location`, `server_type`, `image`, `name`, `private_key_uuid`, `enable_ipv4`, `enable_ipv6`, `enable_backups`, `hetzner_ssh_key_ids`, `hetzner_firewall_ids`, `hetzner_network_ids`, `cloud_init_script`, `instant_validate` |
### Notifications

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/notifications/discord` | `get-current-team-discord-notifications` — Get Discord notification settings | — | — |
| `PATCH` | `/notifications/discord` | `update-current-team-discord-notifications` — Update Discord notification settings | — | — |
| `GET` | `/notifications/email` | `get-current-team-email-notifications` — Get email notification settings | — | — |
| `PATCH` | `/notifications/email` | `update-current-team-email-notifications` — Update email notification settings | — | — |
| `GET` | `/notifications/pushover` | `get-current-team-pushover-notifications` — Get Pushover notification settings | — | — |
| `PATCH` | `/notifications/pushover` | `update-current-team-pushover-notifications` — Update Pushover notification settings | — | — |
| `GET` | `/notifications/slack` | `get-current-team-slack-notifications` — Get Slack notification settings | — | — |
| `PATCH` | `/notifications/slack` | `update-current-team-slack-notifications` — Update Slack notification settings | — | — |
| `GET` | `/notifications/telegram` | `get-current-team-telegram-notifications` — Get Telegram notification settings | — | — |
| `PATCH` | `/notifications/telegram` | `update-current-team-telegram-notifications` — Update Telegram notification settings | — | — |
| `GET` | `/notifications/webhook` | `get-current-team-webhook-notifications` — Get webhook notification settings | — | — |
| `PATCH` | `/notifications/webhook` | `update-current-team-webhook-notifications` — Update webhook notification settings | — | — |
### Private Keys

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/security/keys` | `list-private-keys` — List | — | — |
| `PATCH` | `/security/keys` | `update-private-key` — Update | — | `name`, `description`, `private_key` |
| `POST` | `/security/keys` | `create-private-key` — Create | — | `name`, `description`, `private_key` |
| `DELETE` | `/security/keys/{uuid}` | `delete-private-key-by-uuid` — Delete | `uuid (required)` | — |
| `GET` | `/security/keys/{uuid}` | `get-private-key-by-uuid` — Get | `uuid (required)` | — |
### Projects

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/projects` | `list-projects` — List | — | — |
| `POST` | `/projects` | `create-project` — Create | — | `name`, `description` |
| `DELETE` | `/projects/{uuid}` | `delete-project-by-uuid` — Delete | `uuid (required)` | — |
| `GET` | `/projects/{uuid}` | `get-project-by-uuid` — Get | `uuid (required)` | — |
| `PATCH` | `/projects/{uuid}` | `update-project-by-uuid` — Update | `uuid (required)` | `name`, `description` |
| `GET` | `/projects/{uuid}/environments` | `get-environments` — List Environments | `uuid (required)` | — |
| `POST` | `/projects/{uuid}/environments` | `create-environment` — Create Environment | `uuid (required)` | `name` |
| `DELETE` | `/projects/{uuid}/environments/{environment_name_or_uuid}` | `delete-environment` — Delete Environment | `uuid (required)`, `environment_name_or_uuid (required)` | — |
| `PATCH` | `/projects/{uuid}/environments/{environment_name_or_uuid}` | `update-environment` — Update Environment | `uuid (required)`, `environment_name_or_uuid (required)` | `name`, `description` |
| `GET` | `/projects/{uuid}/{environment_name_or_uuid}` | `get-environment-by-name-or-uuid` — Environment | `uuid (required)`, `environment_name_or_uuid (required)` | — |
### Resources

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/resources` | `list-resources` — List | — | — |
### S3 Storages

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/s3-storages` | `list-s3-storages` — List S3 Storages | — | — |
| `POST` | `/s3-storages` | `create-s3-storage` — Create S3 Storage | — | `name`, `description`, `endpoint`, `bucket`, `region`, `key`, `secret`, `is_usable` |
| `DELETE` | `/s3-storages/{uuid}` | `delete-s3-storage-by-uuid` — Delete S3 Storage | `uuid (required)` | — |
| `GET` | `/s3-storages/{uuid}` | `get-s3-storage-by-uuid` — Get S3 Storage | `uuid (required)` | — |
| `PATCH` | `/s3-storages/{uuid}` | `update-s3-storage-by-uuid` — Update S3 Storage | `uuid (required)` | `name`, `description`, `endpoint`, `bucket`, `region`, `key`, `secret`, `is_usable` |
| `POST` | `/s3-storages/{uuid}/validate` | `validate-s3-storage-by-uuid` — Validate S3 Storage | `uuid (required)` | — |
### Scheduled Tasks

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/applications/{uuid}/scheduled-tasks` | `list-scheduled-tasks-by-application-uuid` — List Tasks | `uuid (required)` | — |
| `POST` | `/applications/{uuid}/scheduled-tasks` | `create-scheduled-task-by-application-uuid` — Create Task | `uuid (required)` | `name`, `command`, `frequency`, `container`, `timeout`, `enabled` |
| `DELETE` | `/applications/{uuid}/scheduled-tasks/{task_uuid}` | `delete-scheduled-task-by-application-uuid` — Delete Task | `uuid (required)`, `task_uuid (required)` | — |
| `PATCH` | `/applications/{uuid}/scheduled-tasks/{task_uuid}` | `update-scheduled-task-by-application-uuid` — Update Task | `uuid (required)`, `task_uuid (required)` | `name`, `command`, `frequency`, `container`, `timeout`, `enabled` |
| `POST` | `/applications/{uuid}/scheduled-tasks/{task_uuid}/execute` | `execute-scheduled-task-by-application-uuid` — Execute Task | `uuid (required)`, `task_uuid (required)` | — |
| `GET` | `/applications/{uuid}/scheduled-tasks/{task_uuid}/executions` | `list-scheduled-task-executions-by-application-uuid` — List Executions | `uuid (required)`, `task_uuid (required)` | — |
| `GET` | `/services/{uuid}/scheduled-tasks` | `list-scheduled-tasks-by-service-uuid` — List Tasks | `uuid (required)` | — |
| `POST` | `/services/{uuid}/scheduled-tasks` | `create-scheduled-task-by-service-uuid` — Create Task | `uuid (required)` | `name`, `command`, `frequency`, `container`, `timeout`, `enabled` |
| `DELETE` | `/services/{uuid}/scheduled-tasks/{task_uuid}` | `delete-scheduled-task-by-service-uuid` — Delete Task | `uuid (required)`, `task_uuid (required)` | — |
| `PATCH` | `/services/{uuid}/scheduled-tasks/{task_uuid}` | `update-scheduled-task-by-service-uuid` — Update Task | `uuid (required)`, `task_uuid (required)` | `name`, `command`, `frequency`, `container`, `timeout`, `enabled` |
| `POST` | `/services/{uuid}/scheduled-tasks/{task_uuid}/execute` | `execute-scheduled-task-by-service-uuid` — Execute Task | `uuid (required)`, `task_uuid (required)` | — |
| `GET` | `/services/{uuid}/scheduled-tasks/{task_uuid}/executions` | `list-scheduled-task-executions-by-service-uuid` — List Executions | `uuid (required)`, `task_uuid (required)` | — |
### Servers

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/servers` | `list-servers` — List | — | — |
| `POST` | `/servers` | `create-server` — Create | — | `name`, `description`, `ip`, `port`, `user`, `private_key_uuid`, `is_build_server`, `instant_validate`, `proxy_type` |
| `POST` | `/servers/import` | `import-server-transfer-bundle` — Import server transfer bundle | — | `bundle`, `passphrase`, `dry_run`, `preserve_uuids`, `adopt_mode`, `claim`, `write_remote`, `rebind_sentinel` |
| `DELETE` | `/servers/{uuid}` | `delete-server-by-uuid` — Delete | `uuid (required)` | — |
| `GET` | `/servers/{uuid}` | `get-server-by-uuid` — Get | `uuid (required)` | — |
| `PATCH` | `/servers/{uuid}` | `update-server-by-uuid` — Update | `uuid (required)` | `name`, `description`, `ip`, `port`, `user`, `private_key_uuid`, `is_build_server`, `instant_validate`, `proxy_type`, `concurrent_builds`, `dynamic_timeout`, `deployment_queue_limit`, `server_disk_usage_notification_threshold`, `server_disk_usage_check_frequency`, `connection_timeout` |
| `POST` | `/servers/{uuid}/claim` | `claim-server` — Claim imported server | `uuid (required)` | `write_remote`, `rebind_sentinel` |
| `GET` | `/servers/{uuid}/cloudflare-tunnel` | `get-server-cloudflare-tunnel` — Get Cloudflare Tunnel settings | `uuid (required)` | — |
| `PATCH` | `/servers/{uuid}/cloudflare-tunnel` | `update-server-cloudflare-tunnel` — Update Cloudflare Tunnel settings | `uuid (required)` | `is_cloudflare_tunnel` |
| `POST` | `/servers/{uuid}/cloudflare-tunnel/disable` | `disable-server-cloudflare-tunnel` — Disable Cloudflare Tunnel | `uuid (required)` | — |
| `POST` | `/servers/{uuid}/cloudflare-tunnel/enable` | `enable-server-cloudflare-tunnel` — Enable Cloudflare Tunnel (manual) | `uuid (required)` | — |
| `GET` | `/servers/{uuid}/docker-cleanup` | `get-server-docker-cleanup` — Get Docker cleanup settings | `uuid (required)` | — |
| `PATCH` | `/servers/{uuid}/docker-cleanup` | `update-server-docker-cleanup` — Update Docker cleanup settings | `uuid (required)` | `docker_cleanup_frequency`, `docker_cleanup_threshold`, `force_docker_cleanup`, `delete_unused_volumes`, `delete_unused_networks`, `disable_application_image_retention` |
| `GET` | `/servers/{uuid}/docker-cleanup/executions` | `list-server-docker-cleanup-executions` — List Docker cleanup executions | `uuid (required)` | — |
| `POST` | `/servers/{uuid}/docker-cleanup/run` | `run-server-docker-cleanup` — Run Docker cleanup | `uuid (required)` | `delete_unused_volumes`, `delete_unused_networks` |
| `GET` | `/servers/{uuid}/domains` | `get-domains-by-server-uuid` — Domains | `uuid (required)` | — |
| `GET` | `/servers/{uuid}/export` | `export-server-transfer-bundle` — Export server transfer bundle | `uuid (required)`, `encrypt`, `passphrase` | — |
| `POST` | `/servers/{uuid}/export/mailbox` | `export-server-transfer-mailbox` — Write transfer bundle to server mailbox | `uuid (required)` | `passphrase` |
| `GET` | `/servers/{uuid}/log-drains` | `get-server-log-drains` — Get log drain settings | `uuid (required)` | — |
| `PATCH` | `/servers/{uuid}/log-drains` | `update-server-log-drains` — Update log drain settings | `uuid (required)` | `is_logdrain_newrelic_enabled`, `logdrain_newrelic_license_key`, `logdrain_newrelic_base_uri`, `is_logdrain_axiom_enabled`, `logdrain_axiom_dataset_name`, `logdrain_axiom_api_key`, `is_logdrain_custom_enabled`, `logdrain_custom_config`, `logdrain_custom_config_parser` |
| `POST` | `/servers/{uuid}/migrate` | `migrate-server-between-instances` — Migrate server to another Coolify instance | `uuid (required)` | `target_url`, `target_token`, `write_remote`, `rebind_sentinel`, `preserve_uuids`, `adopt_mode` |
| `GET` | `/servers/{uuid}/proxy` | `get-server-proxy` — Get server proxy | `uuid (required)` | — |
| `PATCH` | `/servers/{uuid}/proxy` | `update-server-proxy` — Update server proxy | `uuid (required)` | `redirect_enabled`, `redirect_url`, `generate_exact_labels`, `proxy_type` |
| `PUT` | `/servers/{uuid}/proxy/configuration` | `save-server-proxy-configuration` — Save server proxy configuration | `uuid (required)` | `configuration` |
| `POST` | `/servers/{uuid}/proxy/restart` | `restart-server-proxy` — Restart server proxy | `uuid (required)` | — |
| `GET` | `/servers/{uuid}/resources` | `get-resources-by-server-uuid` — Resources | `uuid (required)` | — |
| `GET` | `/servers/{uuid}/sentinel` | `get-server-sentinel` — Get Sentinel settings | `uuid (required)` | — |
| `PATCH` | `/servers/{uuid}/sentinel` | `update-server-sentinel` — Update Sentinel settings | `uuid (required)` | `is_sentinel_enabled`, `is_metrics_enabled`, `is_sentinel_debug_enabled`, `sentinel_token`, `sentinel_metrics_refresh_rate_seconds`, `sentinel_metrics_history_days`, `sentinel_push_interval_seconds`, `sentinel_custom_url` |
| `POST` | `/servers/{uuid}/transfer/complete` | `complete-server-transfer` — Mark server transferred | `uuid (required)` | `export_id`, `target_instance_url` |
| `POST` | `/servers/{uuid}/validate` | `validate-server-by-uuid` — Validate | `uuid (required)` | `install` |
### Service applications

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/services/{uuid}/applications` | `list-service-applications-by-service-uuid` — List service applications | `uuid (required)` | — |
| `GET` | `/services/{uuid}/applications/{app_uuid}` | `get-service-application-by-service-and-app-uuid` — Get service application | `uuid (required)`, `app_uuid (required)` | — |
| `PATCH` | `/services/{uuid}/applications/{app_uuid}` | `patch-service-application-by-service-and-app-uuid` — Update service application | `uuid (required)`, `app_uuid (required)`, `force_domain_override` | `url`, `noindex_domains`, `human_name`, `description`, `image`, `exclude_from_status`, `is_log_drain_enabled`, `is_gzip_enabled`, `is_stripprefix_enabled`, `max_restart_count` |
| `GET` | `/services/{uuid}/applications/{app_uuid}/logs` | `get-service-application-logs-by-service-and-app-uuid` — Get service application logs | `uuid (required)`, `app_uuid (required)`, `lines` | — |
| `POST` | `/services/{uuid}/applications/{app_uuid}/logs` | `post-service-application-logs-by-service-and-app-uuid` — Get service application logs | `uuid (required)`, `app_uuid (required)`, `lines` | — |
| `POST` | `/services/{uuid}/applications/{app_uuid}/restart` | `post-restart-service-application-by-service-and-app-uuid` — Restart service application container | `uuid (required)`, `app_uuid (required)` | — |
| `POST` | `/services/{uuid}/applications/{app_uuid}/start` | `post-start-service-application-by-service-and-app-uuid` — Start or redeploy service application container | `uuid (required)`, `app_uuid (required)`, `force`, `latest` | — |
| `POST` | `/services/{uuid}/applications/{app_uuid}/stop` | `post-stop-service-application-by-service-and-app-uuid` — Stop service application container | `uuid (required)`, `app_uuid (required)` | — |
### Service databases

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/services/{uuid}/databases` | `list-service-databases-by-service-uuid` — List service databases | `uuid (required)` | — |
| `GET` | `/services/{uuid}/databases/{database_uuid}` | `get-service-database-by-service-and-database-uuid` — Get service database | `uuid (required)`, `database_uuid (required)` | — |
| `PATCH` | `/services/{uuid}/databases/{database_uuid}` | `patch-service-database-by-service-and-database-uuid` — Update service database | `uuid (required)`, `database_uuid (required)` | `human_name`, `description`, `image`, `exclude_from_status`, `is_log_drain_enabled`, `is_public`, `public_port`, `public_port_timeout` |
| `GET` | `/services/{uuid}/databases/{database_uuid}/logs` | `get-service-database-logs-by-service-and-database-uuid` — Get service database logs | `uuid (required)`, `database_uuid (required)`, `lines` | — |
| `POST` | `/services/{uuid}/databases/{database_uuid}/restart` | `restart-service-database-by-service-and-database-uuid` — Restart service database container | `uuid (required)`, `database_uuid (required)` | — |
| `POST` | `/services/{uuid}/databases/{database_uuid}/start` | `start-service-database-by-service-and-database-uuid` — Start or redeploy service database container | `uuid (required)`, `database_uuid (required)`, `force`, `latest` | — |
| `POST` | `/services/{uuid}/databases/{database_uuid}/stop` | `stop-service-database-by-service-and-database-uuid` — Stop service database container | `uuid (required)`, `database_uuid (required)` | — |
### Services

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/services` | `list-services` — List | — | — |
| `POST` | `/services` | `create-service` — Create service | — | `type`, `name`, `description`, `project_uuid`, `environment_name`, `environment_uuid`, `server_uuid`, `destination_uuid`, `instant_deploy`, `docker_compose_raw`, `urls`, `force_domain_override`, `is_container_label_escape_enabled`, `tags` |
| `DELETE` | `/services/{uuid}` | `delete-service-by-uuid` — Delete | `uuid (required)`, `delete_configurations`, `delete_volumes`, `docker_cleanup`, `delete_connected_networks` | — |
| `GET` | `/services/{uuid}` | `get-service-by-uuid` — Get | `uuid (required)` | — |
| `PATCH` | `/services/{uuid}` | `update-service-by-uuid` — Update | `uuid (required)` | `name`, `description`, `instant_deploy`, `connect_to_docker_network`, `docker_compose_raw`, `urls`, `force_domain_override`, `is_container_label_escape_enabled` |
| `POST` | `/services/{uuid}/clone` | `clone-service-by-uuid` — Clone | `uuid (required)` | `destination_uuid`, `name`, `clone_volumes` |
| `GET` | `/services/{uuid}/envs` | `list-envs-by-service-uuid` — List Envs | `uuid (required)` | — |
| `PATCH` | `/services/{uuid}/envs` | `update-env-by-service-uuid` — Update Env | `uuid (required)` | `key`, `value`, `is_preview`, `is_literal`, `is_multiline`, `is_shown_once` |
| `POST` | `/services/{uuid}/envs` | `create-env-by-service-uuid` — Create Env | `uuid (required)` | `key`, `value`, `is_preview`, `is_literal`, `is_multiline`, `is_shown_once` |
| `PATCH` | `/services/{uuid}/envs/bulk` | `update-envs-by-service-uuid` — Update Envs (Bulk) | `uuid (required)` | `data` |
| `DELETE` | `/services/{uuid}/envs/{env_uuid}` | `delete-env-by-service-uuid` — Delete Env | `uuid (required)`, `env_uuid (required)` | — |
| `GET` | `/services/{uuid}/logs` | `get-service-logs-by-uuid` — Get service logs. | `uuid (required)`, `sub_service_name (required)`, `lines`, `show_timestamps` | — |
| `POST` | `/services/{uuid}/migrate` | `migrate-service-by-uuid` — Migrate to Server | `uuid (required)` | `destination_uuid`, `migrate_volumes` |
| `POST` | `/services/{uuid}/move` | `move-service-by-uuid` — Move | `uuid (required)` | `environment_uuid` |
| `POST` | `/services/{uuid}/restart` | `restart-service-by-uuid` — Restart | `uuid (required)`, `latest` | — |
| `POST` | `/services/{uuid}/start` | `start-service-by-uuid` — Start | `uuid (required)` | — |
| `POST` | `/services/{uuid}/stop` | `stop-service-by-uuid` — Stop | `uuid (required)`, `docker_cleanup` | — |
| `GET` | `/services/{uuid}/storages` | `list-storages-by-service-uuid` — List Storages | `uuid (required)` | — |
| `PATCH` | `/services/{uuid}/storages` | `update-storage-by-service-uuid` — Update Storage | `uuid (required)` | `uuid`, `id`, `type`, `is_preview_suffix_enabled`, `name`, `mount_path`, `content` |
| `POST` | `/services/{uuid}/storages` | `create-storage-by-service-uuid` — Create Storage | `uuid (required)` | `type`, `resource_uuid`, `name`, `mount_path`, `content`, `is_directory`, `fs_path` |
| `DELETE` | `/services/{uuid}/storages/{storage_uuid}` | `delete-storage-by-service-uuid` — Delete Storage | `uuid (required)`, `storage_uuid (required)` | — |
| `DELETE` | `/services/{uuid}/storages/{storage_uuid}/backups` | `delete-service-storage-backup-schedule` — Delete service storage backup schedule | `uuid (required)`, `storage_uuid (required)` | — |
| `PUT` | `/services/{uuid}/storages/{storage_uuid}/backups` | `set-service-storage-backup-schedule` — Set service storage backup schedule | `uuid (required)`, `storage_uuid (required)` | `VolumeBackupScheduleRequest` |
| `POST` | `/services/{uuid}/storages/{storage_uuid}/backups/run` | `run-service-storage-backup` — Run service storage backup | `uuid (required)`, `storage_uuid (required)` | — |
| `GET` | `/services/{uuid}/tags` | `list-tags-by-service-uuid` — List Tags | `uuid (required)` | — |
| `POST` | `/services/{uuid}/tags` | `create-tag-by-service-uuid` — Create Tag | `uuid (required)` | `tag_name`, `tag_names` |
| `DELETE` | `/services/{uuid}/tags/{tag_uuid}` | `delete-tag-by-service-uuid` — Delete Tag | `uuid (required)`, `tag_uuid (required)` | — |
### Shared Environment Variables

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/projects/{uuid}/environments/{environment_name_or_uuid}/envs` | `list-environment-shared-envs` — List Environment Shared Envs | `uuid (required)`, `environment_name_or_uuid (required)` | — |
| `POST` | `/projects/{uuid}/environments/{environment_name_or_uuid}/envs` | `create-environment-shared-env` — Create Environment Shared Env | `uuid (required)`, `environment_name_or_uuid (required)` | — |
| `DELETE` | `/projects/{uuid}/environments/{environment_name_or_uuid}/envs/{env_id}` | `delete-environment-shared-env` — Delete Environment Shared Env | `uuid (required)`, `environment_name_or_uuid (required)`, `env_id (required)` | — |
| `PATCH` | `/projects/{uuid}/environments/{environment_name_or_uuid}/envs/{env_id}` | `update-environment-shared-env` — Update Environment Shared Env | `uuid (required)`, `environment_name_or_uuid (required)`, `env_id (required)` | — |
| `GET` | `/projects/{uuid}/envs` | `list-project-shared-envs` — List Project Shared Envs | `uuid (required)` | — |
| `POST` | `/projects/{uuid}/envs` | `create-project-shared-env` — Create Project Shared Env | `uuid (required)` | — |
| `DELETE` | `/projects/{uuid}/envs/{env_id}` | `delete-project-shared-env` — Delete Project Shared Env | `uuid (required)`, `env_id (required)` | — |
| `PATCH` | `/projects/{uuid}/envs/{env_id}` | `update-project-shared-env` — Update Project Shared Env | `uuid (required)`, `env_id (required)` | — |
| `GET` | `/servers/{uuid}/envs` | `list-server-shared-envs` — List Server Shared Envs | `uuid (required)` | — |
| `POST` | `/servers/{uuid}/envs` | `create-server-shared-env` — Create Server Shared Env | `uuid (required)` | — |
| `DELETE` | `/servers/{uuid}/envs/{env_id}` | `delete-server-shared-env` — Delete Server Shared Env | `uuid (required)`, `env_id (required)` | — |
| `PATCH` | `/servers/{uuid}/envs/{env_id}` | `update-server-shared-env` — Update Server Shared Env | `uuid (required)`, `env_id (required)` | — |
| `GET` | `/team/envs` | `list-team-shared-envs` — List Team Shared Envs | — | — |
| `POST` | `/team/envs` | `create-team-shared-env` — Create Team Shared Env | — | `key`, `value`, `is_literal`, `is_multiline`, `is_shown_once`, `comment` |
| `DELETE` | `/team/envs/{env_id}` | `delete-team-shared-env` — Delete Team Shared Env | `env_id (required)` | — |
| `PATCH` | `/team/envs/{env_id}` | `update-team-shared-env` — Update Team Shared Env | `env_id (required)` | — |
### Tags

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/tags` | `list-tags` — List | — | — |
| `POST` | `/tags` | `create-tag` — Create | — | `name` |
| `DELETE` | `/tags/{uuid}` | `delete-tag-by-uuid` — Delete | `uuid (required)` | — |
| `PATCH` | `/tags/{uuid}` | `update-tag-by-uuid` — Update | `uuid (required)` | `name` |
### Teams

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `GET` | `/team` | `get-token-team` — Authenticated Team | — | — |
| `GET` | `/team/members` | `get-token-team-members` — Authenticated Team Members | — | — |
| `GET` | `/teams` | `list-teams` — List | — | — |
| `GET` | `/teams/{id}` | `get-team-by-id` — Get | `id (required)` | — |
| `GET` | `/teams/{id}/members` | `get-members-by-team-id` — Members | `id (required)` | — |
### Vultr

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `POST` | `/servers/vultr` | `create-vultr-server` — Create Vultr Server | — | — |
| `GET` | `/vultr/os` | `get-vultr-operating-systems` — Get Vultr Operating Systems | — | — |
| `GET` | `/vultr/plans` | `get-vultr-plans` — Get Vultr Plans | — | — |
| `GET` | `/vultr/regions` | `get-vultr-regions` — Get Vultr Regions | — | — |
| `GET` | `/vultr/ssh-keys` | `get-vultr-ssh-keys` — Get Vultr SSH Keys | — | — |
### System / untagged operations

| Method | Path | Operation | Parameters | JSON body schema |
|---|---|---|---|---|
| `POST` | `/disable` | `disable-api` — Disable API | — | — |
| `POST` | `/enable` | `enable-api` — Enable API | — | — |
| `GET` | `/health` | `healthcheck` — Healthcheck | — | — |
| `POST` | `/mcp/disable` | `disable-mcp` — Disable MCP Server | — | — |
| `POST` | `/mcp/enable` | `enable-mcp` — Enable MCP Server | — | — |
| `GET` | `/version` | `version` — Version | — | — |
