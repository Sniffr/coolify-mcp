# Using the hosted Coolify MCP

One shared endpoint, your own Coolify. Connect with GitHub, save your Coolify
URL + API token once in Settings, then ask Claude to manage your Coolify —
inventory, logs, diagnostics, deploys — from Claude Code, Claude Desktop, or
OpenCode.

- **MCP URL:** `https://mcp.social.dpdns.org/mcp`
- **Settings:** `https://mcp.social.dpdns.org/settings`
- **Health:** `https://mcp.social.dpdns.org/healthz`

Your token is encrypted per GitHub user, never displayed, and never shared
with other users. The server has no global Coolify credential.

## 1. Connect

### Claude Code (CLI)

```bash
claude mcp add --transport http coolify https://mcp.social.dpdns.org/mcp
claude mcp login coolify
```

Complete the GitHub approval in the browser. Then verify:

```bash
claude mcp list            # want: coolify - ✔ Connected
claude mcp get coolify
curl -fsS https://mcp.social.dpdns.org/healthz
curl -fsS https://mcp.social.dpdns.org/.well-known/oauth-authorization-server | head -c 400; echo
```

You want `coolify - ✔ Connected`. Other useful commands:

```bash
claude mcp login coolify    # re-authenticate after logout/expiry
claude mcp logout coolify   # drop the local OAuth credential
claude mcp remove coolify -s local  # remove the server entry
```

### Claude Desktop

Settings → Connectors → add a remote MCP server with URL
`https://mcp.social.dpdns.org/mcp`. Approve the GitHub login in the browser,
then continue at step 2 below.

### OpenCode

Add a remote MCP server with URL `https://mcp.social.dpdns.org/mcp`, run its
OAuth login flow, approve GitHub, then continue at step 2 below.

## 2. Save your Coolify connection

Open `https://mcp.social.dpdns.org/settings` **in the same browser session
you used for GitHub login**, fill in, and press **Save connection**:

| Field | What to enter |
|---|---|
| Base URL | Server root only, e.g. `https://coolify.example.com` or `http://203.0.113.10:8000` |
| API token | From your Coolify dashboard (user menu → API tokens), pasted with no extra spaces |
| Capability | `read-only` unless you need restarts/deploys (`operations`) or full control (`admin`) |

URL rules with examples:

- ✅ `https://coolify.example.com` — ideal.
- ✅ `http://203.0.113.10:8000` — works; the page warns that plain `http://` sends your token unencrypted, so prefer `https://`.
- ❌ `https://coolify.example.com/api/v1` — remove `/api/v1`, it is added automatically (the form now strips it, but save the short form).
- ❌ `coolify.example.com` — missing `http://`/`https://` prefix.
- ❌ `localhost`, `192.168.x.x`, `10.x.x.x`, `*.local` — the MCP server must be able to reach your Coolify over the network, so private/local addresses are rejected. Expose it via a public IP, public DNS, or a tunnel.

After saving, the page shows **Connected to {host}**. Return to Claude and
re-run `claude mcp list` — it should show connected.

## 3. Sample prompts

Start read-only. These all work on the default `read-only` profile:

```text
List my Coolify servers and their status.
Show an infrastructure overview of my Coolify estate.
List all applications and which ones are unhealthy.
Get details for application <name-or-uuid>.
Show the last 100 lines of logs for <app>.
Diagnose why <app> is failing.
Find issues across my Coolify estate.
What Coolify version am I running?
```

Deployments and writes (need `operations` or `admin`, and Claude will ask
for confirmation):

```text
List recent deployments and their status.
Redeploy <app> and watch it finish.
Restart all apps in project <name>.
Show environment variables for <app>.
Check server resources for <server>.
```

Housekeeping:

```text
List my databases and their backup status.
List scheduled tasks across my servers.
Search the Coolify docs for <topic>.
```

Tips:

- Refer to apps/servers by name; Claude resolves them via list/get tools.
- Log lines cap at 10,000 — ask for 50–200 lines for readability.
- Destructive tools (`stop_all_apps`, deletes) always confirm first — never ask Claude to skip confirmation.

### Tool inventory (45 tools)

Read-only profile (default, 23 tools): `get_version`, `get_mcp_version`,
`get_infrastructure_overview`, `list_servers`, `list_applications`,
`list_databases`, `list_services`, `list_deployments`, `get_server`,
`get_application`, `get_database`, `get_service`, `server_resources`,
`server_domains`, `list_destinations`, `diagnose_app`, `diagnose_server`,
`find_issues`, `search_docs`, `application_logs`, `logs`, `teams`
(plus `list_instances` in fleet mode).

`operations`/`admin` add: `application`, `bulk_env_update`, `cloud_tokens`,
`control`, `database`, `database_backups`, `deploy`, `deployment`, `env_vars`,
`environments`, `github_apps`, `hetzner`, `private_keys`, `projects`,
`redeploy_project`, `restart_project_apps`, `scheduled_tasks`, `service`,
`stop_all_apps`, `storages`, `system`, `tags`, `validate_server`.

## 4. Capability profiles

| Profile | Allows | Choose when |
|---|---|---|
| `read-only` (default) | List, inspect, logs, diagnostics | Inventory, monitoring, debugging — start here |
| `operations` | Plus restarts, redeploys, env updates (confirmed) | You deploy/restart via Claude |
| `admin` | Full control incl. destructive actions (confirmed) | You operate the whole estate via Claude |

Change profile any time by re-saving in Settings.

## 5. When something fails

### In Claude (`claude mcp list` / connector status)

| Message | Meaning | Fix |
|---|---|---|
| `Needs authentication` | Server registered, OAuth not done | `claude mcp login coolify`, approve GitHub |
| `No Coolify connection saved… open …/settings` | Logged in, no URL/token stored | Step 2 above, same browser session |
| `GitHub login required…` | Token expired/invalid | `claude mcp logout coolify && claude mcp login coolify` |
| `tools fetch failed` with HTTP text | Anything else | Read the message — it names the cause |

### On the Settings page (red banner)

| Message | Meaning | Fix |
|---|---|---|
| `Browser session expired or … different browser` | Settings can't see the login session | Do GitHub login, then open `/settings` in that same browser (no incognito split) |
| `Form expired (CSRF mismatch)` | Stale form | Reload `/settings`, submit again |
| `must start with http(s)…`, `must be the server root…` | URL shape wrong | See URL rules above |
| `could not be resolved to a public address` | Private/unresolvable host | Use a public IP/DNS or tunnel |
| `Could not reach Coolify at …` | Network/DNS/firewall | Check host, port, firewall from the server side |
| `Coolify rejected the token (HTTP 401/403)` | Bad/revoked token or wrong permissions | Create a fresh token in Coolify, paste again |
| `Coolify answered HTTP …` | Unexpected API response | Verify the URL is the server root, retry |

The token is never echoed in errors, logs, or the page.

## 6. Rotate or disconnect

- **Rotate:** create the new token in Coolify, save it in Settings (replaces the old ciphertext), then revoke the old token in Coolify.
- **Disconnect:** **Delete connection** in Settings removes the ciphertext and revokes your MCP grants. **Log out** ends the browser session and revokes grants but keeps the connection.
- Never paste tokens into chat prompts, issues, or screenshots. If a token leaks, revoke it in Coolify immediately and save a fresh one.

## 7. Notes for `http://` users

Plain-`http` Coolify hosts work, with two caveats:

1. Your API token travels unencrypted between the MCP host and Coolify — anyone on that path can read it. Prefer `https://`.
2. The host must still be publicly reachable (public IP or DNS). LAN-only addresses can't work because validation requires the server to contact Coolify directly.
