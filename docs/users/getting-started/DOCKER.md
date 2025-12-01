---
title: Docker Installation
module: getting-started
id: docker-installation
order: 3
description: Complete Docker deployment guide
tags: [docker, deployment, installation, getting-started]
---

# Docker Installation

Complete guide for deploying Nexus using Docker and Docker Compose.

## Quick Start

### Using Docker Run

```bash
docker run -d \
  --name nexus \
  -p 15474:15474 \
  -v nexus-data:/app/data \
  -e NEXUS_ROOT_USERNAME=admin \
  -e NEXUS_ROOT_PASSWORD=secure_password_here \
  -e NEXUS_AUTH_ENABLED=true \
  ghcr.io/hivellm/nexus:latest
```

### Using Docker Compose

Create `docker-compose.yml`:

```yaml
version: '3.8'

services:
  nexus:
    image: ghcr.io/hivellm/nexus:latest
    container_name: nexus
    ports:
      - "15474:15474"
    volumes:
      - nexus-data:/app/data
    environment:
      - NEXUS_ROOT_USERNAME=admin
      - NEXUS_ROOT_PASSWORD=secure_password_here
      - NEXUS_AUTH_ENABLED=true
      - NEXUS_DISABLE_ROOT_AFTER_SETUP=true
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:15474/health"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  nexus-data:
```

Start with:
```bash
docker-compose up -d
```

## Docker Secrets

For production, use Docker secrets:

```yaml
services:
  nexus:
    image: ghcr.io/hivellm/nexus:latest
    secrets:
      - root_password
    environment:
      - NEXUS_ROOT_PASSWORD_FILE=/run/secrets/root_password

secrets:
  root_password:
    file: ./secrets/root_password.txt
```

Create the secret file:
```bash
echo "your_secure_password" > secrets/root_password.txt
chmod 600 secrets/root_password.txt
```

## Environment Variables

Key environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `NEXUS_BIND_ADDR` | Server bind address | `0.0.0.0:15474` |
| `NEXUS_ROOT_USERNAME` | Root username | `root` |
| `NEXUS_ROOT_PASSWORD` | Root password | `root` |
| `NEXUS_ROOT_ENABLED` | Enable root user | `true` |
| `NEXUS_DISABLE_ROOT_AFTER_SETUP` | Auto-disable root | `false` |
| `NEXUS_AUTH_ENABLED` | Enable authentication | `false` |
| `NEXUS_DATA_DIR` | Data directory | `/app/data` |

## Volumes

### Data Persistence

```yaml
volumes:
  - nexus-data:/app/data
```

### Custom Data Directory

```yaml
volumes:
  - /host/path/to/data:/app/data
```

## Networking

### Expose Ports

```yaml
ports:
  - "15474:15474"  # REST API
```

### Custom Network

```yaml
networks:
  - nexus-network

networks:
  nexus-network:
    driver: bridge
```

## Health Checks

```yaml
healthcheck:
  test: ["CMD", "curl", "-f", "http://localhost:15474/health"]
  interval: 30s
  timeout: 10s
  retries: 3
  start_period: 40s
```

## Resource Limits

```yaml
deploy:
  resources:
    limits:
      cpus: '2'
      memory: 4G
    reservations:
      cpus: '1'
      memory: 2G
```

## Production Deployment

### Security Best Practices

1. **Use Docker Secrets** for passwords
2. **Disable Root User** after setup
3. **Use HTTPS** with reverse proxy
4. **Limit Resources** to prevent DoS
5. **Regular Backups** of data volumes

### Example Production Setup

```yaml
version: '3.8'

services:
  nexus:
    image: ghcr.io/hivellm/nexus:latest
    container_name: nexus
    ports:
      - "127.0.0.1:15474:15474"  # Bind to localhost only
    volumes:
      - nexus-data:/app/data
    secrets:
      - root_password
    environment:
      - NEXUS_ROOT_PASSWORD_FILE=/run/secrets/root_password
      - NEXUS_AUTH_ENABLED=true
      - NEXUS_DISABLE_ROOT_AFTER_SETUP=true
    restart: always
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:15474/health"]
      interval: 30s
      timeout: 10s
      retries: 3
    deploy:
      resources:
        limits:
          cpus: '4'
          memory: 8G
        reservations:
          cpus: '2'
          memory: 4G

volumes:
  nexus-data:
    driver: local

secrets:
  root_password:
    file: ./secrets/root_password.txt
```

## Troubleshooting

### Check Logs

```bash
docker logs nexus
docker logs -f nexus  # Follow logs
```

### Access Container

```bash
docker exec -it nexus /bin/bash
```

### Restart Container

```bash
docker restart nexus
```

### Remove Container

```bash
docker stop nexus
docker rm nexus
```

## Related Topics

- [Installation Guide](./INSTALLATION.md) - General installation
- [Configuration Guide](../configuration/CONFIGURATION.md) - Advanced configuration
- [Service Management](../operations/SERVICE_MANAGEMENT.md) - Managing services

