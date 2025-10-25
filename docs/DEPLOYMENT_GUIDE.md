# Nexus Graph Database - Deployment Guide

## Table of Contents

1. [System Requirements](#system-requirements)
2. [Installation](#installation)
3. [Configuration](#configuration)
4. [Production Deployment](#production-deployment)
5. [Docker Deployment](#docker-deployment)
6. [Kubernetes Deployment](#kubernetes-deployment)
7. [Monitoring](#monitoring)
8. [Backup and Recovery](#backup-and-recovery)
9. [Security](#security)
10. [Troubleshooting](#troubleshooting)

## System Requirements

### Minimum Requirements

- **CPU**: 2 cores, 2.0 GHz
- **RAM**: 4 GB
- **Storage**: 10 GB SSD
- **OS**: Linux (Ubuntu 20.04+), macOS (10.15+), Windows 10+
- **Network**: 100 Mbps

### Recommended Requirements

- **CPU**: 8+ cores, 3.0+ GHz
- **RAM**: 16+ GB
- **Storage**: 100+ GB NVMe SSD
- **OS**: Linux (Ubuntu 22.04 LTS)
- **Network**: 1 Gbps

### Software Dependencies

- Rust 1.70+ (for building from source)
- Docker (for containerized deployment)
- Kubernetes (for orchestrated deployment)

## Installation

### Option 1: Pre-built Binaries

```bash
# Download latest release
wget https://github.com/your-org/nexus/releases/latest/download/nexus-server-linux-x86_64.tar.gz

# Extract and install
tar -xzf nexus-server-linux-x86_64.tar.gz
sudo mv nexus-server /usr/local/bin/
sudo chmod +x /usr/local/bin/nexus-server

# Verify installation
nexus-server --version
```

### Option 2: Build from Source

```bash
# Clone repository
git clone https://github.com/your-org/nexus.git
cd nexus

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Build release version
cargo build --release

# Install
sudo cp target/release/nexus-server /usr/local/bin/
sudo chmod +x /usr/local/bin/nexus-server
```

### Option 3: Package Managers

#### Ubuntu/Debian

```bash
# Add repository (when available)
curl -fsSL https://packages.nexus-db.com/debian/nexus-keyring.gpg | sudo gpg --dearmor -o /usr/share/keyrings/nexus-archive-keyring.gpg
echo "deb [signed-by=/usr/share/keyrings/nexus-archive-keyring.gpg] https://packages.nexus-db.com/debian stable main" | sudo tee /etc/apt/sources.list.d/nexus.list

# Install
sudo apt update
sudo apt install nexus-server
```

#### macOS (Homebrew)

```bash
# Add tap (when available)
brew tap nexus-db/nexus

# Install
brew install nexus-server
```

## Configuration

### Configuration File

Create a configuration file at `/etc/nexus/config.yml`:

```yaml
# Server configuration
server:
  host: "0.0.0.0"
  port: 3000
  workers: 8
  
# Database configuration
database:
  data_dir: "/var/lib/nexus/data"
  catalog_dir: "/var/lib/nexus/catalog"
  wal_dir: "/var/lib/nexus/wal"
  
# Index configuration
indexes:
  label_index:
    enabled: true
    cache_size_mb: 256
    
  knn_index:
    enabled: true
    dimension: 128
    cache_size_mb: 512
    
# Logging configuration
logging:
  level: "info"
  file: "/var/log/nexus/nexus.log"
  max_size_mb: 100
  max_files: 5
  
# Performance tuning
performance:
  max_memory_mb: 8192
  query_timeout_ms: 30000
  batch_size: 1000
  
# Security (for future implementation)
security:
  enabled: false
  jwt_secret: ""
  cors_origins: []
```

### Environment Variables

You can also configure Nexus using environment variables:

```bash
export NEXUS_HOST=0.0.0.0
export NEXUS_PORT=3000
export NEXUS_DATA_DIR=/var/lib/nexus/data
export NEXUS_LOG_LEVEL=info
export NEXUS_MAX_MEMORY_MB=8192
```

### System Service

Create a systemd service file at `/etc/systemd/system/nexus.service`:

```ini
[Unit]
Description=Nexus Graph Database Server
After=network.target

[Service]
Type=simple
User=nexus
Group=nexus
ExecStart=/usr/local/bin/nexus-server --config /etc/nexus/config.yml
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=nexus

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/nexus /var/log/nexus

# Resource limits
LimitNOFILE=65536
LimitNPROC=32768

[Install]
WantedBy=multi-user.target
```

Enable and start the service:

```bash
# Create nexus user
sudo useradd -r -s /bin/false -d /var/lib/nexus nexus

# Create directories
sudo mkdir -p /var/lib/nexus/{data,catalog,wal}
sudo mkdir -p /var/log/nexus
sudo mkdir -p /etc/nexus

# Set permissions
sudo chown -R nexus:nexus /var/lib/nexus /var/log/nexus
sudo chmod 755 /var/lib/nexus /var/log/nexus

# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable nexus
sudo systemctl start nexus

# Check status
sudo systemctl status nexus
```

## Production Deployment

### High Availability Setup

For production deployments, consider the following architecture:

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Load Balancer │    │   Load Balancer │    │   Load Balancer │
│     (HAProxy)   │    │     (HAProxy)   │    │     (HAProxy)   │
└─────────┬───────┘    └─────────┬───────┘    └─────────┬───────┘
          │                      │                      │
          ▼                      ▼                      ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Nexus Node 1  │    │   Nexus Node 2  │    │   Nexus Node 3  │
│   (Primary)     │    │   (Secondary)   │    │   (Secondary)   │
└─────────────────┘    └─────────────────┘    └─────────────────┘
          │                      │                      │
          └──────────────────────┼──────────────────────┘
                                 ▼
                    ┌─────────────────────────┐
                    │    Shared Storage       │
                    │    (NFS/Ceph)          │
                    └─────────────────────────┘
```

### Load Balancer Configuration (HAProxy)

```haproxy
global
    daemon
    maxconn 4096

defaults
    mode http
    timeout connect 5000ms
    timeout client 50000ms
    timeout server 50000ms

frontend nexus_frontend
    bind *:80
    bind *:443 ssl crt /etc/ssl/certs/nexus.pem
    redirect scheme https if !{ ssl_fc }
    
    default_backend nexus_backend

backend nexus_backend
    balance roundrobin
    option httpchk GET /health
    
    server nexus1 10.0.1.10:3000 check
    server nexus2 10.0.1.11:3000 check
    server nexus3 10.0.1.12:3000 check
```

### Database Replication

For data replication between nodes, implement a custom replication strategy:

```bash
# On primary node
nexus-server --role primary --replication-port 3001

# On secondary nodes
nexus-server --role secondary --primary-host 10.0.1.10 --primary-port 3001
```

## Docker Deployment

### Dockerfile

```dockerfile
FROM rust:1.70-slim as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/nexus-server /usr/local/bin/nexus-server

RUN useradd -r -s /bin/false nexus
USER nexus

EXPOSE 3000

VOLUME ["/var/lib/nexus"]

CMD ["nexus-server", "--config", "/etc/nexus/config.yml"]
```

### Docker Compose

```yaml
version: '3.8'

services:
  nexus:
    build: .
    ports:
      - "3000:3000"
    volumes:
      - nexus_data:/var/lib/nexus
      - ./config.yml:/etc/nexus/config.yml:ro
    environment:
      - NEXUS_LOG_LEVEL=info
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  nexus-replica:
    build: .
    ports:
      - "3001:3000"
    volumes:
      - nexus_replica_data:/var/lib/nexus
      - ./config-replica.yml:/etc/nexus/config.yml:ro
    environment:
      - NEXUS_LOG_LEVEL=info
    restart: unless-stopped
    depends_on:
      - nexus

volumes:
  nexus_data:
  nexus_replica_data:
```

### Docker Run

```bash
# Build image
docker build -t nexus-server .

# Run container
docker run -d \
  --name nexus \
  -p 3000:3000 \
  -v nexus_data:/var/lib/nexus \
  -v ./config.yml:/etc/nexus/config.yml:ro \
  nexus-server
```

## Kubernetes Deployment

### Namespace

```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: nexus
```

### ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: nexus-config
  namespace: nexus
data:
  config.yml: |
    server:
      host: "0.0.0.0"
      port: 3000
      workers: 4
    database:
      data_dir: "/var/lib/nexus/data"
    logging:
      level: "info"
```

### PersistentVolumeClaim

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: nexus-data
  namespace: nexus
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 100Gi
  storageClassName: fast-ssd
```

### Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nexus
  namespace: nexus
spec:
  replicas: 3
  selector:
    matchLabels:
      app: nexus
  template:
    metadata:
      labels:
        app: nexus
    spec:
      containers:
      - name: nexus
        image: nexus-server:latest
        ports:
        - containerPort: 3000
        volumeMounts:
        - name: config
          mountPath: /etc/nexus
        - name: data
          mountPath: /var/lib/nexus
        env:
        - name: NEXUS_LOG_LEVEL
          value: "info"
        resources:
          requests:
            memory: "2Gi"
            cpu: "500m"
          limits:
            memory: "8Gi"
            cpu: "2"
        livenessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 5
          periodSeconds: 5
      volumes:
      - name: config
        configMap:
          name: nexus-config
      - name: data
        persistentVolumeClaim:
          claimName: nexus-data
```

### Service

```yaml
apiVersion: v1
kind: Service
metadata:
  name: nexus-service
  namespace: nexus
spec:
  selector:
    app: nexus
  ports:
  - port: 3000
    targetPort: 3000
  type: ClusterIP
```

### Ingress

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: nexus-ingress
  namespace: nexus
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
spec:
  tls:
  - hosts:
    - nexus.example.com
    secretName: nexus-tls
  rules:
  - host: nexus.example.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: nexus-service
            port:
              number: 3000
```

## Monitoring

### Prometheus Metrics

Nexus exposes Prometheus-compatible metrics at `/metrics`:

```yaml
# Add to prometheus.yml
scrape_configs:
  - job_name: 'nexus'
    static_configs:
      - targets: ['nexus.example.com:3000']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

### Grafana Dashboard

Key metrics to monitor:

- **Query Performance**: Query execution time, QPS
- **Memory Usage**: Heap usage, cache hit rates
- **Storage**: Disk usage, WAL size
- **Network**: Request rate, error rate
- **Indexes**: Index size, search performance

### Log Aggregation

Configure centralized logging with ELK stack or similar:

```yaml
# Logstash configuration
input {
  beats {
    port => 5044
  }
}

filter {
  if [fields][service] == "nexus" {
    grok {
      match => { "message" => "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}" }
    }
  }
}

output {
  elasticsearch {
    hosts => ["elasticsearch:9200"]
    index => "nexus-%{+YYYY.MM.dd}"
  }
}
```

## Backup and Recovery

### Backup Strategy

```bash
#!/bin/bash
# backup-nexus.sh

BACKUP_DIR="/backup/nexus"
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="nexus_backup_${DATE}.tar.gz"

# Create backup directory
mkdir -p $BACKUP_DIR

# Stop Nexus service
systemctl stop nexus

# Create backup
tar -czf "${BACKUP_DIR}/${BACKUP_FILE}" \
  /var/lib/nexus/data \
  /var/lib/nexus/catalog \
  /var/lib/nexus/wal

# Start Nexus service
systemctl start nexus

# Upload to cloud storage (optional)
aws s3 cp "${BACKUP_DIR}/${BACKUP_FILE}" s3://nexus-backups/

# Cleanup old backups (keep last 7 days)
find $BACKUP_DIR -name "nexus_backup_*.tar.gz" -mtime +7 -delete

echo "Backup completed: ${BACKUP_FILE}"
```

### Recovery Process

```bash
#!/bin/bash
# restore-nexus.sh

BACKUP_FILE=$1
RESTORE_DIR="/var/lib/nexus"

if [ -z "$BACKUP_FILE" ]; then
    echo "Usage: $0 <backup_file>"
    exit 1
fi

# Stop Nexus service
systemctl stop nexus

# Restore from backup
tar -xzf "$BACKUP_FILE" -C /

# Start Nexus service
systemctl start nexus

echo "Recovery completed from: $BACKUP_FILE"
```

### Automated Backups

Set up automated backups with cron:

```bash
# Add to crontab
0 2 * * * /opt/nexus/scripts/backup-nexus.sh
```

## Security

### Network Security

1. **Firewall Configuration**:
```bash
# Allow only necessary ports
ufw allow 3000/tcp
ufw deny 3001/tcp  # Replication port (internal only)
```

2. **SSL/TLS Configuration**:
```yaml
# In config.yml
server:
  tls:
    enabled: true
    cert_file: "/etc/ssl/certs/nexus.crt"
    key_file: "/etc/ssl/private/nexus.key"
```

### Authentication (Future Implementation)

```yaml
# Planned security configuration
security:
  enabled: true
  jwt_secret: "your-secret-key"
  cors_origins:
    - "https://app.example.com"
  rate_limiting:
    enabled: true
    requests_per_minute: 1000
```

### Data Encryption

```bash
# Encrypt data directory
sudo cryptsetup luksFormat /dev/sdb
sudo cryptsetup luksOpen /dev/sdb nexus_data
sudo mkfs.ext4 /dev/mapper/nexus_data
sudo mount /dev/mapper/nexus_data /var/lib/nexus
```

## Troubleshooting

### Common Issues

#### High Memory Usage

```bash
# Check memory usage
free -h
ps aux --sort=-%mem | head

# Adjust memory limits in config.yml
performance:
  max_memory_mb: 4096  # Reduce if needed
```

#### Slow Queries

```bash
# Enable query logging
logging:
  level: "debug"
  query_logging: true

# Check slow queries in logs
tail -f /var/log/nexus/nexus.log | grep "slow query"
```

#### Disk Space Issues

```bash
# Check disk usage
df -h
du -sh /var/lib/nexus/*

# Clean up old WAL files
find /var/lib/nexus/wal -name "*.wal" -mtime +7 -delete
```

#### Connection Issues

```bash
# Check if service is running
systemctl status nexus

# Check port binding
netstat -tlnp | grep 3000

# Test connectivity
curl -f http://localhost:3000/health
```

### Performance Tuning

#### System-level Optimizations

```bash
# Increase file descriptor limits
echo "nexus soft nofile 65536" >> /etc/security/limits.conf
echo "nexus hard nofile 65536" >> /etc/security/limits.conf

# Optimize kernel parameters
echo "vm.swappiness=10" >> /etc/sysctl.conf
echo "vm.dirty_ratio=15" >> /etc/sysctl.conf
sysctl -p
```

#### Application-level Optimizations

```yaml
# In config.yml
performance:
  max_memory_mb: 8192
  query_timeout_ms: 30000
  batch_size: 1000
  cache_size_mb: 1024

indexes:
  label_index:
    cache_size_mb: 512
  knn_index:
    cache_size_mb: 1024
```

### Support and Maintenance

#### Log Analysis

```bash
# Monitor error logs
tail -f /var/log/nexus/nexus.log | grep ERROR

# Analyze query performance
grep "execution_time" /var/log/nexus/nexus.log | sort -k3 -n
```

#### Health Checks

```bash
# Create health check script
#!/bin/bash
response=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:3000/health)
if [ $response -eq 200 ]; then
    echo "Nexus is healthy"
    exit 0
else
    echo "Nexus is unhealthy (HTTP $response)"
    exit 1
fi
```

This deployment guide provides comprehensive instructions for deploying Nexus in various environments. For additional support, refer to the troubleshooting section or contact the development team.
