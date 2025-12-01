---
title: Server Configuration
module: configuration
id: server-configuration
order: 2
description: Network, ports, host binding, reverse proxy
tags: [server, network, ports, configuration]
---

# Server Configuration

Complete guide for configuring Nexus server settings.

## Network Binding

### Bind Address

```bash
# Bind to all interfaces
export NEXUS_BIND_ADDR="0.0.0.0:15474"

# Bind to localhost only
export NEXUS_BIND_ADDR="127.0.0.1:15474"

# Bind to specific interface
export NEXUS_BIND_ADDR="192.168.1.100:15474"
```

### Config File

```yaml
server:
  bind_addr: "0.0.0.0:15474"
```

## Port Configuration

### Default Port

- **Default**: `15474`
- **REST API**: `15474`
- **MCP**: `15474/mcp`
- **UMICP**: `15474/umicp`

### Change Port

```bash
export NEXUS_BIND_ADDR="0.0.0.0:8080"
```

Or in config file:
```yaml
server:
  bind_addr: "0.0.0.0:8080"
```

## Reverse Proxy

### Nginx Configuration

```nginx
server {
    listen 80;
    server_name nexus.example.com;

    location / {
        proxy_pass http://localhost:15474;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Caddy Configuration

```
nexus.example.com {
    reverse_proxy localhost:15474
}
```

### Traefik Configuration

```yaml
services:
  nexus:
    labels:
      - "traefik.http.routers.nexus.rule=Host(`nexus.example.com`)"
      - "traefik.http.services.nexus.loadbalancer.server.port=15474"
```

## SSL/TLS Configuration

### Using Reverse Proxy

Configure SSL at the reverse proxy level (Nginx, Caddy, Traefik).

### Nginx SSL Example

```nginx
server {
    listen 443 ssl;
    server_name nexus.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://localhost:15474;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## Connection Limits

### Max Connections

```yaml
server:
  max_connections: 1000
  connection_timeout_seconds: 30
```

### Environment Variables

```bash
export NEXUS_MAX_CONNECTIONS=1000
export NEXUS_CONNECTION_TIMEOUT_SECONDS=30
```

## CORS Configuration

### Enable CORS

```yaml
server:
  cors:
    enabled: true
    allowed_origins:
      - "https://example.com"
      - "https://app.example.com"
    allowed_methods:
      - "GET"
      - "POST"
      - "PUT"
      - "DELETE"
```

## Related Topics

- [Configuration Overview](./CONFIGURATION.md) - General configuration
- [Logging Configuration](./LOGGING.md) - Log management
- [Performance Tuning](./PERFORMANCE_TUNING.md) - Performance optimization

