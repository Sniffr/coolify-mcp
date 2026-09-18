#!/usr/bin/env bash
set -eu

REPO_URL="${COOLIFY_MCP_REPO_URL:-https://raw.githubusercontent.com/Sniffr/coolify-mcp/main}"
INSTALL_DIR="${COOLIFY_MCP_INSTALL_DIR:-${HOME}/.local/share/coolify-mcp}"
ENV_FILE="${COOLIFY_MCP_ENV_FILE:-${HOME}/.config/coolify-mcp/env}"
MCP_NAME="${COOLIFY_MCP_NAME:-coolify}"

say() { printf '\033[1;36m[Coolify MCP]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[Coolify MCP] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "curl is required"
command -v python3 >/dev/null 2>&1 || die "python3 is required"
command -v claude >/dev/null 2>&1 || die "Claude Code CLI not found. Install it first, then rerun."

mkdir -p "$INSTALL_DIR" "$(dirname "$ENV_FILE")"
chmod 700 "$(dirname "$ENV_FILE")"
curl --fail --silent --show-error --location "$REPO_URL/coolify_mcp_server.py" -o "$INSTALL_DIR/coolify_mcp_server.py"
chmod 700 "$INSTALL_DIR" "$INSTALL_DIR/coolify_mcp_server.py"

if [ ! -f "$ENV_FILE" ]; then
  umask 077
  {
    printf '# Coolify MCP credentials — chmod 600; do not commit\n'
    printf 'COOLIFY_URL=%s\n' "${COOLIFY_URL:-https://your-coolify.example}"
    printf 'COOLIFY_TOKEN=%s\n' "${COOLIFY_TOKEN:-replace-with-complete-token}"
  } > "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  say "Created private environment file: $ENV_FILE"
else
  chmod 600 "$ENV_FILE"
fi

# Claude Code's official MCP CLI registers a local stdio server.
# The wrapper loads the private env file before starting the Python process.
cat > "$INSTALL_DIR/run.sh" <<EOF
#!/usr/bin/env bash
set -a
. "$ENV_FILE"
set +a
exec python3 "$INSTALL_DIR/coolify_mcp_server.py"
EOF
chmod 700 "$INSTALL_DIR/run.sh"

claude mcp remove "$MCP_NAME" -s user >/dev/null 2>&1 || true
claude mcp add --transport stdio --scope user "$MCP_NAME" -- "$INSTALL_DIR/run.sh"
say "Installed MCP server: $MCP_NAME"
say "Edit credentials: $ENV_FILE"
say "Then verify with: claude mcp list"
say "Restart Claude Code after installation."
