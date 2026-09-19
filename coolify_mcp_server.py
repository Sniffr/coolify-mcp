#!/usr/bin/env python3
"""Minimal stdio MCP server for Coolify's REST API.

Credentials are read only from COOLIFY_URL and COOLIFY_TOKEN environment variables.
"""
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request


# MCP stdio clients (e.g. Claude Code) send newline-delimited JSON; some others use
# Content-Length framing. The framing of the first message is used for all replies.
FRAMING = "content-length"


def write_message(message):
    raw = json.dumps(message, separators=(",", ":")).encode()
    if FRAMING == "newline":
        sys.stdout.buffer.write(raw + b"\n")
    else:
        sys.stdout.buffer.write(f"Content-Length: {len(raw)}\r\n\r\n".encode() + raw)
    sys.stdout.buffer.flush()


def read_message():
    global FRAMING
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if not headers and line.lstrip().startswith(b"{"):
            FRAMING = "newline"
            return json.loads(line)
        if line in (b"\r\n", b"\n"):
            if not headers:
                continue
            break
        key, value = line.decode("ascii").split(":", 1)
        headers[key.lower()] = value.strip()
    length = int(headers.get("content-length", "0"))
    return json.loads(sys.stdin.buffer.read(length))


def base_url():
    value = os.environ.get("COOLIFY_URL", "").strip().rstrip("/")
    if not value:
        raise RuntimeError("COOLIFY_URL is not configured")
    parsed = urllib.parse.urlparse(value)
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        raise RuntimeError("COOLIFY_URL must be an http(s) URL")
    if parsed.path.rstrip("/").endswith("/api/v1"):
        return value
    return value + "/api/v1"


def request_coolify(method, path, query=None, body=None):
    method = method.upper()
    if method not in {"GET", "POST", "PATCH", "PUT", "DELETE"}:
        raise ValueError("method must be GET, POST, PATCH, PUT, or DELETE")
    if not isinstance(path, str) or not path.startswith("/") or path.startswith("//") or ".." in path:
        raise ValueError("path must be a relative path under /api/v1")
    if path == "/health":
        url = base_url() + path
    else:
        url = base_url() + path
    if query:
        url += "?" + urllib.parse.urlencode(query, doseq=True)
    token = os.environ.get("COOLIFY_TOKEN", "")
    payload = None if body is None else json.dumps(body).encode()
    headers = {"Accept": "application/json"}
    if path != "/health":
        if not token:
            raise RuntimeError("COOLIFY_TOKEN is not configured")
        headers["Authorization"] = f"Bearer {token}"
    if payload is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=payload, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=45) as response:
            raw = response.read()
            status = response.status
            response_headers = response.headers
    except urllib.error.HTTPError as error:
        raw = error.read()
        status = error.code
        response_headers = error.headers
    content_type = response_headers.get("Content-Type", "")
    try:
        result = json.loads(raw) if raw else None
    except json.JSONDecodeError:
        result = raw.decode("utf-8", errors="replace")
    return {
        "status": status,
        "ok": 200 <= status < 300,
        "headers": {
            "retry_after": response_headers.get("Retry-After"),
            "rate_limit": response_headers.get("X-RateLimit-Limit"),
            "rate_remaining": response_headers.get("X-RateLimit-Remaining"),
            "rate_reset": response_headers.get("X-RateLimit-Reset"),
            "content_type": content_type,
        },
        "body": result,
    }


TOOLS = [
    {
        "name": "coolify_request",
        "description": "Call a documented Coolify API operation. Use read-only calls first. The client must obtain user confirmation before POST/PATCH/PUT/DELETE, deployments, restarts, stops, cancellations, production changes, or sensitive reads. path is relative to /api/v1; never provide a hostname or token.",
        "inputSchema": {
            "type": "object", "required": ["method", "path"],
            "properties": {
                "method": {"type": "string", "enum": ["GET", "POST", "PATCH", "PUT", "DELETE"]},
                "path": {"type": "string", "description": "Documented path such as /applications or /deploy"},
                "query": {"type": "object", "additionalProperties": True},
                "body": {"type": ["object", "array", "string", "number", "boolean", "null"]},
            },
        },
    },
    {
        "name": "coolify_health",
        "description": "Check whether Coolify is reachable. This uses the public health endpoint and does not require a token.",
        "inputSchema": {"type": "object", "properties": {}},
    },
]


def handle(message):
    request_id = message.get("id")
    method = message.get("method")
    if method == "initialize":
        return {"jsonrpc": "2.0", "id": request_id, "result": {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}, "serverInfo": {"name": "coolify-mcp", "version": "1.0.0"}}}
    if method == "notifications/initialized":
        return None
    if method == "tools/list":
        return {"jsonrpc": "2.0", "id": request_id, "result": {"tools": TOOLS}}
    if method == "tools/call":
        args = message.get("params", {}).get("arguments", {})
        name = message.get("params", {}).get("name")
        try:
            if name == "coolify_health":
                result = request_coolify("GET", "/health")
            elif name == "coolify_request":
                result = request_coolify(args.get("method", ""), args.get("path", ""), args.get("query"), args.get("body"))
            else:
                raise ValueError(f"unknown tool: {name}")
            text = json.dumps(result, indent=2)
            return {"jsonrpc": "2.0", "id": request_id, "result": {"content": [{"type": "text", "text": text}], "isError": not result["ok"]}}
        except Exception as error:
            return {"jsonrpc": "2.0", "id": request_id, "result": {"content": [{"type": "text", "text": json.dumps({"error": str(error)})}], "isError": True}}
    if request_id is not None:
        return {"jsonrpc": "2.0", "id": request_id, "error": {"code": -32601, "message": f"Unknown method: {method}"}}
    return None


def main():
    while True:
        message = read_message()
        if message is None:
            break
        response = handle(message)
        if response:
            write_message(response)


if __name__ == "__main__":
    main()
