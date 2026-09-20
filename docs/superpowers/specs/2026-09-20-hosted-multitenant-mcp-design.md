# Hosted Multi-User Coolify MCP Bridge Design

**Date:** 2026-09-20  
**Status:** Design approved in chat; awaiting written-spec review  
**Scope:** Convert the hosted Rust HTTP service from a shared server-side Coolify connection into a GitHub-authenticated, per-user Coolify bridge.

## 1. Goal and user experience

The service provides one remote MCP endpoint:

```text
https://mcp.social.dpdns.org/mcp
```

A user should be able to:

1. Add that URL to Claude, OpenCode, or another OAuth-capable MCP client.
2. Complete GitHub login in the browser.
3. Open the service settings page.
4. Enter their own Coolify base URL and API token.
5. Use MCP tools against only their own Coolify connection.

The service must not use one global Coolify token for all users. The existing local stdio mode remains available for users who prefer to keep the token entirely on their own machine.

## 2. Trust and tenancy model

- GitHub user identity is the tenant boundary.
- Every authenticated user has an internal stable user ID derived from the GitHub account ID.
- Each MCP access/refresh grant is bound to one internal user ID.
- Each Coolify connection is bound to one internal user ID.
- A request may load only the connection belonging to the authenticated MCP grant.
- User-controlled identifiers, URLs, arguments, tool results, and Coolify responses must not select or reveal another tenant's records.
- Open registration is allowed: any GitHub account may create its own isolated account.
- There is no shared public Coolify credential.

The hosted server necessarily holds encrypted copies of users' Coolify tokens because ordinary remote MCP clients do not forward arbitrary local environment variables to the HTTP service. The local stdio mode remains the no-server-storage alternative.

## 3. Authentication and OAuth flow

The current self-issued OAuth authorization flow is replaced or extended with an upstream GitHub identity flow:

1. MCP client begins authorization with the service using OAuth authorization code + PKCE.
2. The service validates the MCP client, redirect URI, resource, and PKCE request.
3. If there is no authenticated browser session, the service redirects to GitHub with minimal read-only identity scopes.
4. GitHub returns to a fixed HTTPS callback on the service.
5. The service validates GitHub state, exchanges the GitHub code server-side, and obtains the GitHub account ID and display identity.
6. The service creates or loads the local user record.
7. The original MCP authorization transaction resumes and issues an authorization code bound to that user, client, redirect URI, resource, and PKCE challenge.
8. The MCP client exchanges the code for a bearer access token and refresh token.
9. The HTTP MCP endpoint accepts only a valid bearer token bound to the exact `/mcp` resource and user grant.

Security requirements:

- GitHub client secret is server-side only.
- GitHub and MCP state values are authenticated, single-use, expiring, and stored as hashes where persistence is required.
- PKCE S256 is mandatory.
- Redirect URI and resource binding remain exact.
- Browser session cookies are Secure, HttpOnly, SameSite, short-lived, and rotated as appropriate.
- Browser settings mutations require CSRF protection.
- Logout, refresh-token replay, connection deletion, and account deletion revoke the affected user grants.
- GitHub access tokens are not persisted unless strictly required; if persisted, they are encrypted and minimized.
- No OAuth codes, tokens, cookies, GitHub secrets, or Coolify tokens may appear in logs, URLs, audit events, errors, images, or responses.

## 4. User settings and Coolify connection

The first version exposes one default Coolify connection per user, with a schema that can support multiple named connections later.

Settings operations:

- Create or replace the user's default connection.
- Validate the Coolify URL format without logging it beyond a safe host identifier.
- Store the Coolify token encrypted at rest.
- Run a bounded, safe read-only validation request only after explicit user submission.
- Delete and rotate the connection.
- Show only non-secret metadata: Coolify hostname, connection status, last validation time, and selected capability profile.

The settings page must never redisplay the token. Forms must not put the token in the URL. API responses must return a redacted success object and must never echo submitted secrets.

## 5. Encrypted persistence

Use a persistent SQLite database in `/data` for users, GitHub identity mapping, encrypted Coolify connections, sessions, and tenant-bound MCP grants. Existing OAuth state and audit persistence remain under `/data`.

Required deployment secret:

```text
MCP_CONNECTION_ENCRYPTION_KEY
```

The key must be a high-entropy value supplied through the host secret mechanism. It must not be generated from a predictable value, committed, printed, or included in an image layer.

Token encryption requirements:

- AEAD encryption such as AES-256-GCM or ChaCha20-Poly1305.
- Fresh random nonce per stored value.
- Associated data includes the internal user ID and connection purpose.
- Decryption failures fail closed for that connection and do not expose ciphertext or plaintext.
- Database and key files are mode 0600; `/data` is mode 0700.
- Writes are transactional and crash-safe.
- Backups must be treated as containing encrypted secrets and protected accordingly.

## 6. MCP capability policy

- HTTP default is `read-only`.
- Each user's selected profile is stored with the connection, defaulting to `read-only`.
- Read-only tools are available without confirmation.
- Operations/admin profiles require explicit user configuration and existing confirmation checks for writes, deletes, and deployments.
- The server must never silently elevate a user's profile based on client input.
- Coolify API authorization failures remain errors; they must not be converted into broader access.
- Tool audit records include safe user/client/tool/outcome identifiers and duration, but never arguments, responses, or credentials.

## 7. HTTP routes

Public or browser-authenticated routes will include:

- OAuth discovery and protected-resource metadata.
- MCP OAuth registration and authorization endpoints.
- GitHub sign-in start and callback endpoints.
- Authenticated settings page and connection-management endpoints.
- Public health endpoint with degraded status when persistence or audit is unavailable.
- Bearer-protected `/mcp` Streamable HTTP endpoint.

Caddy terminates HTTPS for `mcp.social.dpdns.org` and proxies the service privately to the MCP container. The container is attached to the existing Caddy network but is not directly published on a host port.

## 8. Deployment configuration

The hosted deployment has no global `COOLIFY_BASE_URL` or `COOLIFY_ACCESS_TOKEN`. Required runtime configuration is limited to service identity and persistence, including:

- `MCP_TRANSPORT=http`
- `MCP_PUBLIC_URL=https://mcp.social.dpdns.org`
- `MCP_PORT=8080`
- `MCP_CAPABILITY_PROFILE=read-only`
- `MCP_DATABASE_PATH=/data/tenant.sqlite`
- `MCP_CONNECTION_ENCRYPTION_KEY`
- `GITHUB_CLIENT_ID`
- `GITHUB_CLIENT_SECRET`
- `GITHUB_CALLBACK_URL=https://mcp.social.dpdns.org/auth/github/callback`
- persistent `/data`
- persistent audit log and OAuth state paths

Secrets are supplied through a protected host file or equivalent secret manager, never through Git, Dockerfile instructions, image layers, shell history, or public configuration.

## 9. Migration and rollback

- Preserve the current local stdio server and Python fallback.
- Do not start the hosted container with the old shared-token configuration.
- Existing Caddy route may remain configured, but useful traffic must remain blocked or return a controlled unavailable response until the multi-user service is deployed and health-checked.
- Keep the OAuth signing key, state, database, and audit files on persistent storage.
- Rollback restores the previous Caddy configuration and stops the hosted container without deleting encrypted tenant data.
- If encryption-key or database migration fails, fail closed for affected connections and report a generic health/deployment error.

## 10. Testing and acceptance

Add tests for:

- GitHub OAuth state, callback validation, account creation, and failure paths.
- MCP PKCE transaction resumption after GitHub login.
- Tenant isolation across users, MCP grants, settings, and Coolify requests.
- Encryption/decryption, nonce uniqueness, wrong-key failure, crash-safe persistence, and secret-free serialization.
- Settings create/replace/delete behavior and token non-disclosure.
- Read-only defaults, profile boundaries, confirmation requirements, and revoked grants.
- HTTP authorization rejecting missing, invalid, expired, wrong-user, and wrong-resource bearer tokens.
- Audit records for successful, rejected, and failed calls without secrets or arguments.
- End-to-end fake GitHub + fake Coolify acceptance with two users proving each sees only its own fixture data.
- Caddy/HTTPS health, OAuth discovery, MCP initialization, exact tool listing, and a safe per-user inventory call.

## 11. Documentation deliverables

Update the README and hosting guide with a dedicated "Hosted multi-user setup" section covering:

1. Registering a GitHub OAuth application.
2. Required callback URL and environment variables.
3. Creating the protected encryption key and persistent `/data` volume.
4. Deploying the container and Caddy route.
5. Adding `https://mcp.social.dpdns.org/mcp` to Claude/OpenCode.
6. Completing GitHub OAuth from the MCP client.
7. Opening Settings and adding a personal Coolify URL/token.
8. Selecting read-only or an explicitly approved higher profile.
9. Revoking and rotating a Coolify connection.
10. Recovery and rollback to local stdio/Python mode.

Examples must use placeholders only and must explicitly warn users never to paste real Coolify or GitHub secrets into chat, Git, images, or logs.
