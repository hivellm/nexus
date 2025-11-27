# Docker Deployment Guide

This guide covers deploying Nexus Graph Database using Docker, including authentication setup, Docker secrets, environment variables, and production security recommendations.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Dockerfile Overview](#dockerfile-overview)
3. [Docker Compose](#docker-compose)
4. [Root User Configuration](#root-user-configuration)
5. [Docker Secrets](#docker-secrets)
6. [Environment Variables](#environment-variables)
7. [Production Deployment](#production-deployment)
8. [Security Recommendations](#security-recommendations)
9. [Troubleshooting](#troubleshooting)

## Quick Start

### Using Docker Compose (Recommended)

1. **Create secrets directory:**
   ```bash
   mkdir -p secrets
   echo "your_secure_password_here" > secrets/root_password.txt
   chmod 600 secrets/root_password.txt
   ```

2. **Start Nexus:**
   ```bash
   docker-compose up -d
   ```

3. **Check logs:**
   ```bash
   docker-compose logs -f nexus
   ```

4. **Verify health:**
   ```bash
   curl http://localhost:15474/health
   ```

### Using Docker Run

```bash
# Build image
docker build -t nexus-graph-db:latest .

# Run container
docker run -d \
  --name nexus \
  -p 15474:15474 \
  -v nexus-data:/app/data \
  -e NEXUS_ROOT_USERNAME=admin \
  -e NEXUS_ROOT_PASSWORD=secure_password_here \
  -e NEXUS_AUTH_ENABLED=true \
  -e NEXUS_DISABLE_ROOT_AFTER_SETUP=true \
  nexus-graph-db:latest
```

## Dockerfile Overview

The Dockerfile uses a multi-stage build for optimal image size:

### Build Stage
- Uses `rustlang/rust:nightly` base image
- Installs build dependencies (pkg-config, libssl-dev)
- Builds the project in release mode
- Produces optimized binary

### Runtime Stage
- Uses `debian:bookworm-slim` for minimal size
- Installs only runtime dependencies (ca-certificates, libssl3)
- Creates non-root user (`nexus`) for security
- Sets up data and config directories
- Includes health check

### Image Size
- Build stage: ~2GB (temporary)
- Runtime stage: ~150MB (final image)

## Docker Compose

The `docker-compose.yml` file provides a complete setup with:

- **Volume persistence**: Data stored in `nexus-data` volume
- **Secrets management**: Root password from Docker secrets
- **Health checks**: Automatic health monitoring
- **Restart policy**: Automatic restart on failure
- **Environment variables**: Configurable via `.env` file

### Customizing Docker Compose

Create a `.env` file:

```bash
# .env
NEXUS_ROOT_USERNAME=admin
NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password
NEXUS_ROOT_ENABLED=true
NEXUS_DISABLE_ROOT_AFTER_SETUP=true
NEXUS_AUTH_ENABLED=true
NEXUS_AUTH_REQUIRED_FOR_PUBLIC=true
RUST_LOG=info
```

## Root User Configuration

### Method 1: Environment Variables (Development)

```bash
docker run -d \
  -e NEXUS_ROOT_USERNAME=admin \
  -e NEXUS_ROOT_PASSWORD=secure_password \
  -e NEXUS_ROOT_ENABLED=true \
  -e NEXUS_DISABLE_ROOT_AFTER_SETUP=true \
  nexus-graph-db:latest
```

### Method 2: Docker Secrets (Production - Recommended)

```bash
# Create secret file
echo "secure_password_here" > secrets/root_password.txt
chmod 600 secrets/root_password.txt

# Use in docker-compose.yml
secrets:
  - nexus_root_password

environment:
  - NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password
```

### Method 3: Config File

Mount `config/auth.toml`:

```bash
docker run -d \
  -v $(pwd)/config:/app/config:ro \
  nexus-graph-db:latest
```

**Priority Order:**
1. Environment variables (highest priority)
2. Docker secrets file (`NEXUS_ROOT_PASSWORD_FILE`)
3. Config file (`config/auth.toml`)
4. Default values (lowest priority)

## Docker Secrets

Docker secrets provide secure credential management:

### Creating Secrets

**Using Docker Swarm:**
```bash
echo "secure_password" | docker secret create nexus_root_password -
```

**Using Docker Compose:**
```yaml
secrets:
  nexus_root_password:
    file: ./secrets/root_password.txt
```

### Using Secrets

**In docker-compose.yml:**
```yaml
services:
  nexus:
    secrets:
      - nexus_root_password
    environment:
      - NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password
```

**In Dockerfile:**
```dockerfile
# Secrets are automatically mounted at /run/secrets/
ENV NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password
```

### Security Best Practices

1. **Never commit secrets to git:**
   ```bash
   echo "secrets/" >> .gitignore
   echo "*.txt" >> .gitignore  # if storing in files
   ```

2. **Use proper file permissions:**
   ```bash
   chmod 600 secrets/root_password.txt
   ```

3. **Rotate secrets regularly:**
   ```bash
   # Update secret file
   echo "new_password" > secrets/root_password.txt
   # Restart container
   docker-compose restart nexus
   ```

## Environment Variables

### Root User Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `NEXUS_ROOT_USERNAME` | `root` | Root user username |
| `NEXUS_ROOT_PASSWORD` | `root` | Root user password (plaintext) |
| `NEXUS_ROOT_PASSWORD_FILE` | - | Path to password file (Docker secrets) |
| `NEXUS_ROOT_ENABLED` | `true` | Enable root user |
| `NEXUS_DISABLE_ROOT_AFTER_SETUP` | `false` | Auto-disable root after first admin |

### Authentication Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `NEXUS_AUTH_ENABLED` | `false` | Enable authentication |
| `NEXUS_AUTH_REQUIRED_FOR_PUBLIC` | `true` | Require auth for 0.0.0.0 binding |
| `NEXUS_AUTH_REQUIRE_HEALTH_AUTH` | `false` | Require auth for /health endpoint |

### Server Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `NEXUS_ADDR` | `127.0.0.1:15474` | Server bind address |
| `NEXUS_DATA_DIR` | `./data` | Data directory path |
| `RUST_LOG` | `info` | Logging level (trace, debug, info, warn, error) |

### Example Configuration

```bash
# Production configuration
export NEXUS_ROOT_USERNAME=admin
export NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password
export NEXUS_ROOT_ENABLED=true
export NEXUS_DISABLE_ROOT_AFTER_SETUP=true
export NEXUS_AUTH_ENABLED=true
export NEXUS_AUTH_REQUIRED_FOR_PUBLIC=true
export NEXUS_ADDR=0.0.0.0:15474
export NEXUS_DATA_DIR=/app/data
export RUST_LOG=warn
```

## Production Deployment

### Step 1: Build Production Image

```bash
# Build optimized image
docker build -t nexus-graph-db:v0.11.0 .

# Tag as latest
docker tag nexus-graph-db:v0.11.0 nexus-graph-db:latest
```

### Step 2: Create Secrets

```bash
# Create secrets directory
mkdir -p secrets

# Generate secure password
openssl rand -base64 32 > secrets/root_password.txt
chmod 600 secrets/root_password.txt
```

### Step 3: Configure Environment

Create `.env` file:

```bash
NEXUS_ROOT_USERNAME=admin
NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password
NEXUS_ROOT_ENABLED=true
NEXUS_DISABLE_ROOT_AFTER_SETUP=true
NEXUS_AUTH_ENABLED=true
NEXUS_AUTH_REQUIRED_FOR_PUBLIC=true
NEXUS_ADDR=0.0.0.0:15474
RUST_LOG=warn
```

### Step 4: Deploy

```bash
# Start services
docker-compose up -d

# Verify deployment
docker-compose ps
docker-compose logs nexus

# Check health
curl http://localhost:15474/health
```

### Step 5: Initial Setup

1. **Login as root:**
   ```bash
   curl -X POST http://localhost:15474/auth/login \
     -H "Content-Type: application/json" \
     -d '{"username": "admin", "password": "your_password"}'
   ```

2. **Create admin user:**
   ```bash
   curl -X POST http://localhost:15474/auth/users \
     -H "Authorization: Bearer YOUR_JWT_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "username": "admin_user",
       "password": "secure_password",
       "permissions": ["READ", "WRITE", "ADMIN"]
     }'
   ```

3. **Root user will auto-disable** (if `NEXUS_DISABLE_ROOT_AFTER_SETUP=true`)

4. **Create API key:**
   ```bash
   curl -X POST http://localhost:15474/auth/api-keys \
     -H "Authorization: Bearer YOUR_JWT_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "name": "production-api-key",
       "permissions": ["READ", "WRITE"]
     }'
   ```

## Security Recommendations

### 1. Use Docker Secrets

**✅ Recommended:**
```yaml
secrets:
  - nexus_root_password
environment:
  - NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password
```

**❌ Avoid:**
```yaml
environment:
  - NEXUS_ROOT_PASSWORD=plaintext_password  # Visible in docker inspect
```

### 2. Enable Authentication for Public Binding

**✅ Required for production:**
```bash
NEXUS_AUTH_ENABLED=true
NEXUS_AUTH_REQUIRED_FOR_PUBLIC=true
```

### 3. Auto-Disable Root User

**✅ Recommended:**
```bash
NEXUS_DISABLE_ROOT_AFTER_SETUP=true
```

This automatically disables the root user after the first admin user is created.

### 4. Use Non-Default Ports

**✅ Recommended:**
```bash
# Map to non-standard port
ports:
  - "127.0.0.1:15474:15474"  # Only accessible from localhost
```

### 5. Limit Container Resources

```yaml
services:
  nexus:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 4G
        reservations:
          cpus: '1'
          memory: 2G
```

### 6. Use Read-Only Root Filesystem

```yaml
services:
  nexus:
    read_only: true
    tmpfs:
      - /tmp
      - /app/data
```

### 7. Network Isolation

```yaml
services:
  nexus:
    networks:
      - internal
    # Don't expose ports publicly

networks:
  internal:
    driver: bridge
```

### 8. Regular Updates

```bash
# Pull latest image
docker pull nexus-graph-db:latest

# Update container
docker-compose pull
docker-compose up -d
```

## Troubleshooting

### Container Won't Start

**Check logs:**
```bash
docker-compose logs nexus
docker logs nexus
```

**Common issues:**
- Port already in use: Change `NEXUS_ADDR` or port mapping
- Permission denied: Check volume permissions
- Missing secrets: Verify secret file exists and is readable

### Authentication Not Working

**Verify configuration:**
```bash
# Check environment variables
docker exec nexus env | grep NEXUS

# Check secret file
docker exec nexus cat /run/secrets/nexus_root_password

# Test login
curl -X POST http://localhost:15474/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your_password"}'
```

### Data Persistence Issues

**Check volume:**
```bash
# List volumes
docker volume ls

# Inspect volume
docker volume inspect nexus-data

# Check data directory
docker exec nexus ls -la /app/data
```

### Health Check Failing

**Manual health check:**
```bash
docker exec nexus curl -f http://localhost:15474/health
```

**Check server logs:**
```bash
docker-compose logs -f nexus
```

### Performance Issues

**Monitor resources:**
```bash
docker stats nexus
```

**Adjust resources:**
```yaml
services:
  nexus:
    deploy:
      resources:
        limits:
          cpus: '4'
          memory: 8G
```

## Additional Resources

- [Authentication Guide](AUTHENTICATION.md) - Complete authentication documentation
- [Security Audit](SECURITY_AUDIT.md) - Security best practices and audit report
- [API Documentation](api/openapi.yml) - Complete API reference

## Support

For issues and questions:
- GitHub Issues: https://github.com/hivellm/nexus/issues
- Documentation: https://github.com/hivellm/nexus/docs
