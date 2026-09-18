FROM python:3.12-slim

WORKDIR /app
COPY coolify_mcp_server.py /app/coolify_mcp_server.py

ENV PYTHONUNBUFFERED=1
USER nobody
ENTRYPOINT ["python3", "/app/coolify_mcp_server.py"]
