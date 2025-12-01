---
title: Docker Authentication
module: api
id: docker-authentication
order: 8
description: Docker authentication setup
tags: [docker, authentication, security]
---

# Docker Authentication

Complete guide for authentication setup in Docker deployments.

## Overview

Setting up authentication in Docker requires careful handling of credentials and secrets.

## Environment Variables

### Basic Setup

```bash
docker run -d \
  --name nexus \
  -p 15474:15474 \
  -e NEXUS_ROOT_USERNAME=admin \
  -e NEXUS_ROOT_PASSWORD=secure_password \
  -e NEXUS_AUTH_ENABLED=true \
  nexus-graph-db:latest
```

## Docker Secrets

### Using Docker Secrets

```yaml
version: '3.8'

services:
  nexus:
    image: ghcr.io/hivellm/nexus:latest
    secrets:
      - root_password
    environment:
      - NEXUS_ROOT_PASSWORD_FILE=/run/secrets/root_password
      - NEXUS_AUTH_ENABLED=true

secrets:
  root_password:
    file: ./secrets/root_password.txt
```

### Create Secret File

```bash
# Create secret
echo "your_secure_password" > secrets/root_password.txt
chmod 600 secrets/root_password.txt
```

## Docker Compose with Secrets

```yaml
version: '3.8'

services:
  nexus:
    image: ghcr.io/hivellm/nexus:latest
    secrets:
      - root_password
      - api_key
    environment:
      - NEXUS_ROOT_PASSWORD_FILE=/run/secrets/root_password
      - NEXUS_API_KEY_FILE=/run/secrets/api_key
      - NEXUS_AUTH_ENABLED=true
      - NEXUS_DISABLE_ROOT_AFTER_SETUP=true

secrets:
  root_password:
    file: ./secrets/root_password.txt
  api_key:
    file: ./secrets/api_key.txt
```

## Production Best Practices

1. **Use Docker Secrets**: Never pass passwords via environment variables in production
2. **Disable Root After Setup**: Set `NEXUS_DISABLE_ROOT_AFTER_SETUP=true`
3. **Use Strong Passwords**: Minimum 12 characters
4. **Rotate Secrets**: Regularly rotate passwords and API keys
5. **Limit Access**: Use network policies to limit access

## Related Topics

- [Docker Installation](../getting-started/DOCKER.md) - Docker deployment
- [Authentication Guide](./AUTHENTICATION.md) - Authentication setup
- [Security Audit](../../SECURITY_AUDIT.md) - Security best practices

