---
title: Operations
module: operations
id: operations-index
order: 0
description: Service management, monitoring, and troubleshooting
tags: [operations, service, monitoring, troubleshooting]
---

# Operations

Service management, monitoring, and troubleshooting guides.

## Guides

### [Service Management](./SERVICE_MANAGEMENT.md)

Managing Nexus services:
- Linux systemd service
- Windows Service
- Service commands
- Auto-start configuration

### [Log Management](./LOGS.md)

Viewing and analyzing logs:
- Log locations
- Log filtering
- Log rotation
- Log analysis

### [Monitoring](./MONITORING.md)

Health checks and metrics:
- Health check endpoints
- Prometheus metrics
- Grafana dashboards
- Alerting

### [Backup and Restore](./BACKUP.md)

Data protection:
- Backup procedures
- Restore operations
- Backup scheduling
- Disaster recovery

### [Troubleshooting](./TROUBLESHOOTING.md)

Common problems and solutions:
- Common issues
- Error messages
- Performance problems
- Debugging tips

## Quick Reference

### Service Commands

**Linux:**
```bash
sudo systemctl status nexus
sudo systemctl start nexus
sudo systemctl stop nexus
sudo systemctl restart nexus
```

**Windows:**
```powershell
Get-Service Nexus
Start-Service Nexus
Stop-Service Nexus
Restart-Service Nexus
```

### Health Check

```bash
curl http://localhost:15474/health
```

### View Logs

**Linux:**
```bash
sudo journalctl -u nexus -f
```

**Windows:**
```powershell
Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100 -Wait
```

## Related Topics

- [Configuration Guide](../configuration/CONFIGURATION.md) - Server configuration
- [Installation Guide](../getting-started/INSTALLATION.md) - Installation instructions
- [Performance Guide](../guides/PERFORMANCE.md) - Performance optimization

