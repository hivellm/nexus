---
title: Data Directory
module: configuration
id: data-directory
order: 4
description: Storage paths, snapshots, migration
tags: [data, storage, directory, configuration]
---

# Data Directory

Complete guide for configuring Nexus data storage.

## Default Locations

### Linux

- **Default**: `/var/lib/nexus/data`
- **Config**: `/etc/nexus/config.yml`

### Windows

- **Default**: `C:\ProgramData\Nexus\data`
- **Config**: `C:\ProgramData\Nexus\config.yml`

### Docker

- **Default**: `/app/data` (inside container)
- **Mount**: Use volumes for persistence

## Configuration

### Set Data Directory

**Environment Variable:**
```bash
export NEXUS_DATA_DIR="/custom/path/to/data"
```

**Config File:**
```yaml
server:
  data_dir: "/custom/path/to/data"
```

## Directory Structure

```
data/
├── catalog/          # LMDB catalog
│   ├── data.mdb
│   └── lock.mdb
├── nodes.store       # Node records
├── rels.store        # Relationship records
├── properties.store   # Property records
├── wal.log           # Write-ahead log
├── checkpoints/      # Checkpoint snapshots
└── indexes/          # Index files
    ├── label_*.bitmap
    └── hnsw_*.bin
```

## Multi-Database Structure

```
data/
├── neo4j/            # Default database
│   ├── catalog/
│   ├── nodes.store
│   └── ...
├── mydb/             # Custom database
│   ├── catalog/
│   ├── nodes.store
│   └── ...
└── ...
```

## Disk Space Management

### Check Disk Usage

```bash
# Check data directory size
du -sh /var/lib/nexus/data

# Check per-database size
du -sh /var/lib/nexus/data/*
```

### Cleanup

```bash
# Remove old checkpoints
find /var/lib/nexus/data/checkpoints -name "*.ckpt" -mtime +7 -delete

# Compact WAL (if supported)
nexus-cli compact-wal
```

## Migration

### Move Data Directory

```bash
# Stop service
sudo systemctl stop nexus

# Copy data
cp -r /var/lib/nexus/data /new/location/data

# Update config
# Set NEXUS_DATA_DIR="/new/location/data"

# Start service
sudo systemctl start nexus
```

### Backup Before Migration

```bash
# Create backup
tar -czf nexus-backup.tar.gz /var/lib/nexus/data

# Restore if needed
tar -xzf nexus-backup.tar.gz -C /new/location
```

## Permissions

### Linux

```bash
# Set ownership
sudo chown -R nexus:nexus /var/lib/nexus/data

# Set permissions
sudo chmod -R 755 /var/lib/nexus/data
```

### Windows

Ensure the service account has full control over the data directory.

## Related Topics

- [Configuration Overview](./CONFIGURATION.md) - General configuration
- [Backup Guide](../operations/BACKUP.md) - Backup procedures
- [Performance Tuning](./PERFORMANCE_TUNING.md) - Performance optimization

