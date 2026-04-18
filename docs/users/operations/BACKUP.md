---
title: Backup and Restore
module: operations
id: backup-restore
order: 4
description: Backup procedures and restore operations
tags: [backup, restore, data-protection, disaster-recovery]
---

# Backup and Restore

Complete guide for backing up and restoring Nexus data.

## Backup Methods

### File System Backup

**Backup data directory:**

```bash
# Stop service
sudo systemctl stop nexus

# Backup data directory
tar -czf nexus-backup-$(date +%Y%m%d).tar.gz /var/lib/nexus/data

# Start service
sudo systemctl start nexus
```

### Online Backup

```bash
# Backup while running (if supported)
nexus-cli backup --output /backup/nexus-backup.tar.gz
```

## Backup Strategy

### Full Backup

```bash
#!/bin/bash
BACKUP_DIR="/backup/nexus"
DATE=$(date +%Y%m%d)
BACKUP_FILE="$BACKUP_DIR/nexus-full-$DATE.tar.gz"

# Create backup directory
mkdir -p $BACKUP_DIR

# Stop service
sudo systemctl stop nexus

# Backup
tar -czf $BACKUP_FILE /var/lib/nexus/data

# Start service
sudo systemctl start nexus

# Keep only last 7 days
find $BACKUP_DIR -name "nexus-full-*.tar.gz" -mtime +7 -delete
```

### Incremental Backup

```bash
#!/bin/bash
BACKUP_DIR="/backup/nexus"
DATE=$(date +%Y%m%d)
INCREMENTAL_FILE="$BACKUP_DIR/nexus-incremental-$DATE.tar.gz"

# Create incremental backup
tar -czf $INCREMENTAL_FILE --newer-mtime="1 day ago" /var/lib/nexus/data
```

## Restore Operations

### Restore from Backup

```bash
# Stop service
sudo systemctl stop nexus

# Restore backup
tar -xzf nexus-backup-20250101.tar.gz -C /

# Verify permissions
chown -R nexus:nexus /var/lib/nexus/data

# Start service
sudo systemctl start nexus
```

### Verify Restore

```bash
# Check health
curl http://localhost:15474/health

# Check statistics
curl http://localhost:15474/stats

# Verify data
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n) RETURN COUNT(n) AS total"}'
```

## Automated Backups

### Cron Job

```bash
# Add to crontab
0 2 * * * /usr/local/bin/nexus-backup.sh
```

### Backup Script

```bash
#!/bin/bash
# nexus-backup.sh

BACKUP_DIR="/backup/nexus"
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/nexus-$DATE.tar.gz"
RETENTION_DAYS=7

# Create backup directory
mkdir -p $BACKUP_DIR

# Stop service
sudo systemctl stop nexus

# Backup
tar -czf $BACKUP_FILE /var/lib/nexus/data

# Start service
sudo systemctl start nexus

# Cleanup old backups
find $BACKUP_DIR -name "nexus-*.tar.gz" -mtime +$RETENTION_DAYS -delete

# Log backup
echo "$(date): Backup completed: $BACKUP_FILE" >> /var/log/nexus-backup.log
```

## Database-Specific Backups

### Backup Specific Database

```bash
# Switch to database
:USE mydb

# Export data (if export feature available)
# Or backup specific database directory
tar -czf mydb-backup.tar.gz /var/lib/nexus/data/mydb
```

## Disaster Recovery

### Recovery Procedure

1. **Stop Service**
   ```bash
   sudo systemctl stop nexus
   ```

2. **Restore Backup**
   ```bash
   tar -xzf nexus-backup-20250101.tar.gz -C /
   ```

3. **Verify Permissions**
   ```bash
   chown -R nexus:nexus /var/lib/nexus/data
   ```

4. **Start Service**
   ```bash
   sudo systemctl start nexus
   ```

5. **Verify Data**
   ```bash
   curl http://localhost:15474/health
   curl http://localhost:15474/stats
   ```

## Best Practices

1. **Regular Backups**: Schedule daily backups
2. **Test Restores**: Regularly test restore procedures
3. **Off-Site Storage**: Store backups off-site
4. **Retention Policy**: Keep backups for appropriate duration
5. **Documentation**: Document backup and restore procedures
6. **Monitoring**: Monitor backup success/failure

## Related Topics

- [Service Management](./SERVICE_MANAGEMENT.md) - Managing services
- [Configuration Guide](../configuration/CONFIGURATION.md) - Server configuration
- [Troubleshooting](./TROUBLESHOOTING.md) - Common problems

