---
title: Service Management
module: operations
id: service-management
order: 1
description: Managing Nexus services on Linux and Windows
tags: [service, systemd, windows-service, management]
---

# Service Management

Complete guide for managing Nexus services on Linux and Windows.

## Linux (systemd)

### Service Status

```bash
# Check status
sudo systemctl status nexus

# Check if running
sudo systemctl is-active nexus

# Check if enabled
sudo systemctl is-enabled nexus
```

### Starting and Stopping

```bash
# Start service
sudo systemctl start nexus

# Stop service
sudo systemctl stop nexus

# Restart service
sudo systemctl restart nexus

# Reload configuration
sudo systemctl reload nexus
```

### Enable/Disable Auto-Start

```bash
# Enable auto-start on boot
sudo systemctl enable nexus

# Disable auto-start
sudo systemctl disable nexus
```

### Viewing Logs

```bash
# View recent logs
sudo journalctl -u nexus

# Follow logs
sudo journalctl -u nexus -f

# View last 100 lines
sudo journalctl -u nexus -n 100

# View logs since today
sudo journalctl -u nexus --since today
```

### Service Configuration

Service file location: `/etc/systemd/system/nexus.service`

```ini
[Unit]
Description=Nexus Graph Database
After=network.target

[Service]
Type=simple
User=nexus
ExecStart=/usr/local/bin/nexus-server
Restart=always
RestartSec=10
Environment="RUST_LOG=info"
Environment="NEXUS_DATA_DIR=/var/lib/nexus/data"

[Install]
WantedBy=multi-user.target
```

### Reload Configuration

After modifying service file:

```bash
sudo systemctl daemon-reload
sudo systemctl restart nexus
```

## Windows

### Service Status

```powershell
# Check status
Get-Service -Name Nexus

# Check if running
(Get-Service -Name Nexus).Status
```

### Starting and Stopping

```powershell
# Start service
Start-Service -Name Nexus

# Stop service
Stop-Service -Name Nexus

# Restart service
Restart-Service -Name Nexus
```

### Enable/Disable Auto-Start

```powershell
# Enable auto-start
Set-Service -Name Nexus -StartupType Automatic

# Disable auto-start
Set-Service -Name Nexus -StartupType Manual
```

### Viewing Logs

```powershell
# View logs
Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100

# Follow logs
Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100 -Wait
```

### Service Configuration

Service is installed via installer script. Configuration is in:
- Service executable: `C:\Program Files\Nexus\nexus-server.exe`
- Config file: `C:\ProgramData\Nexus\config.yml`
- Data directory: `C:\ProgramData\Nexus\data`
- Logs: `C:\ProgramData\Nexus\logs\`

## Docker

### Using Docker Compose

```bash
# Start service
docker-compose up -d

# Stop service
docker-compose down

# Restart service
docker-compose restart

# View logs
docker-compose logs -f nexus
```

### Using Docker Run

```bash
# Start container
docker start nexus

# Stop container
docker stop nexus

# Restart container
docker restart nexus

# View logs
docker logs -f nexus
```

## Health Checks

### Check Service Health

```bash
# Health endpoint
curl http://localhost:15474/health

# Statistics
curl http://localhost:15474/stats
```

### Expected Response

```json
{
  "status": "healthy",
  "version": "0.12.0",
  "uptime_seconds": 12345
}
```

## Troubleshooting

### Service Won't Start

1. **Check Logs:**
   ```bash
   # Linux
   sudo journalctl -u nexus -n 100
   
   # Windows
   Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100
   ```

2. **Check Port:**
   ```bash
   # Linux
   lsof -i :15474
   
   # Windows
   netstat -ano | findstr :15474
   ```

3. **Check Permissions:**
   ```bash
   # Linux
   ls -la /var/lib/nexus/data
   ```

### Service Crashes

1. **Check Resource Limits:**
   ```bash
   # Linux
   systemctl show nexus | grep Limit
   ```

2. **Check Disk Space:**
   ```bash
   df -h
   ```

3. **Check Memory:**
   ```bash
   free -h
   ```

## Related Topics

- [Installation Guide](../getting-started/INSTALLATION.md) - Installation instructions
- [Configuration Guide](../configuration/CONFIGURATION.md) - Server configuration
- [Troubleshooting](./TROUBLESHOOTING.md) - Common problems

