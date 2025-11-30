# Nexus CLI Best Practices

## Security

### Credential Management

**DO:**
- Use environment variables for credentials in scripts
- Use configuration profiles for different environments
- Store API keys in secure credential managers
- Rotate API keys regularly
- Use least-privilege permissions for API keys

**DON'T:**
- Pass passwords as command-line arguments (visible in process list)
- Commit credentials to version control
- Share API keys between environments
- Use root credentials for application access

```bash
# Good: Use environment variables
export NEXUS_API_KEY="your-key"
nexus query "MATCH (n) RETURN n"

# Good: Use config profiles
nexus --profile production query "MATCH (n) RETURN n"

# Bad: Password in command line
nexus --password secret query "MATCH (n) RETURN n"  # Avoid this!
```

### API Key Best Practices

1. **Create specific keys for each application**
   ```bash
   nexus key create app-backend --permissions read,write
   nexus key create analytics --permissions read
   nexus key create admin-tools --permissions read,write,admin
   ```

2. **Revoke unused keys promptly**
   ```bash
   nexus key list
   nexus key revoke old-key-id
   ```

3. **Use minimal permissions**
   - Read-only for reporting/analytics
   - Read-write for applications
   - Admin only for management scripts

## Performance

### Query Optimization

1. **Use LIMIT for exploratory queries**
   ```bash
   # Good: Limited results
   nexus query "MATCH (n) RETURN n LIMIT 100"

   # Bad: May return millions of rows
   nexus query "MATCH (n) RETURN n"
   ```

2. **Use parameters for repeated queries**
   ```bash
   # Good: Parameterized (can be cached)
   nexus query "MATCH (n) WHERE n.id = \$id RETURN n" --params '{"id": 123}'

   # Less optimal: Inline values
   nexus query "MATCH (n) WHERE n.id = 123 RETURN n"
   ```

3. **Use indexes for frequently queried properties**
   ```bash
   nexus query "CREATE INDEX FOR (p:Person) ON (p.email)"
   ```

### Batch Operations

1. **Use batch imports for large datasets**
   ```bash
   nexus data import large-file.json --format json --batch-size 1000
   ```

2. **Export data in chunks for very large databases**
   ```bash
   # Export by label
   nexus query "MATCH (n:Person) RETURN n" > persons.json
   nexus query "MATCH (n:Company) RETURN n" > companies.json
   ```

## Scripting

### Error Handling

```bash
#!/bin/bash
set -e  # Exit on error

# Check server availability first
if ! nexus db ping; then
    echo "Error: Cannot connect to Nexus server"
    exit 1
fi

# Proceed with operations
nexus query "CREATE (n:Test {name: 'example'})"
```

### Using Exit Codes

```bash
#!/bin/bash

nexus query "MATCH (n) RETURN count(n)" 2>/dev/null
case $? in
    0) echo "Query successful" ;;
    1) echo "General error" ;;
    2) echo "Connection error - check server" ;;
    3) echo "Authentication error - check credentials" ;;
    4) echo "Query error - check syntax" ;;
esac
```

### JSON Processing with jq

```bash
# Get node count as number
count=$(nexus --json query "MATCH (n) RETURN count(n) as count" | jq -r '.rows[0][0]')
echo "Database has $count nodes"

# Process query results
nexus --json query "MATCH (p:Person) RETURN p.name, p.email" | \
    jq -r '.rows[] | "\(.[0]): \(.[1])"'
```

### CSV Export for Reporting

```bash
# Export to CSV for spreadsheet analysis
nexus --csv query "
    MATCH (p:Person)-[:WORKS_AT]->(c:Company)
    RETURN p.name, c.name, p.role
    ORDER BY c.name
" > employees.csv
```

## Configuration

### Profile Organization

Organize profiles by environment:

```toml
# ~/.config/nexus/config.toml

# Default settings
url = "http://localhost:3000"

[profiles.local]
url = "http://localhost:3000"
username = "dev"
password = "dev-password"

[profiles.staging]
url = "https://staging.example.com:3000"
api_key = "staging-api-key"

[profiles.production]
url = "https://prod.example.com:3000"
api_key = "prod-api-key"
```

### Environment-Specific Scripts

```bash
#!/bin/bash
# deploy-data.sh

ENV=${1:-staging}

case $ENV in
    staging)
        export NEXUS_PROFILE=staging
        ;;
    production)
        export NEXUS_PROFILE=production
        # Extra confirmation for production
        read -p "Deploy to PRODUCTION? (yes/no): " confirm
        [[ "$confirm" != "yes" ]] && exit 1
        ;;
    *)
        export NEXUS_PROFILE=local
        ;;
esac

nexus data import data.json --format json
```

## Monitoring

### Health Checks

```bash
#!/bin/bash
# health-check.sh - Run periodically via cron

LOG_FILE="/var/log/nexus-health.log"

check_health() {
    local status=$(nexus --json admin health 2>/dev/null)
    if [[ $? -eq 0 ]]; then
        echo "$(date): OK" >> "$LOG_FILE"
    else
        echo "$(date): FAIL" >> "$LOG_FILE"
        # Send alert (email, Slack, etc.)
    fi
}

check_health
```

### Statistics Collection

```bash
#!/bin/bash
# collect-stats.sh

nexus --json admin stats | jq '{
    timestamp: now | todate,
    nodes: .node_count,
    relationships: .relationship_count,
    labels: .label_count
}' >> /var/log/nexus-stats.jsonl
```

## Backup Strategies

### Regular Backups

```bash
#!/bin/bash
# backup.sh

BACKUP_DIR="/backups/nexus"
DATE=$(date +%Y%m%d-%H%M%S)

mkdir -p "$BACKUP_DIR"

# Export all data
nexus data export "$BACKUP_DIR/nexus-$DATE.json" --format json

# Keep only last 7 days
find "$BACKUP_DIR" -name "*.json" -mtime +7 -delete

echo "Backup completed: nexus-$DATE.json"
```

### Verification

```bash
#!/bin/bash
# verify-backup.sh

BACKUP_FILE=$1

# Check file exists and has content
if [[ ! -s "$BACKUP_FILE" ]]; then
    echo "Error: Backup file is empty or missing"
    exit 1
fi

# Validate JSON
if ! jq empty "$BACKUP_FILE" 2>/dev/null; then
    echo "Error: Invalid JSON in backup"
    exit 1
fi

# Check node count
nodes=$(jq '.nodes | length' "$BACKUP_FILE")
echo "Backup contains $nodes nodes"
```

## Troubleshooting

### Debug Mode

```bash
# Enable verbose output for debugging
nexus --verbose --debug db info

# Check connection issues
nexus --verbose db ping
```

### Common Issues

1. **Connection refused**
   - Check server is running
   - Verify URL and port
   - Check firewall rules

2. **Authentication failed**
   - Verify credentials
   - Check API key is active
   - Ensure correct profile is selected

3. **Query timeout**
   - Add LIMIT to reduce result size
   - Check for missing indexes
   - Review query complexity

### Log Analysis

```bash
# Check for patterns in verbose output
nexus --verbose query "..." 2>&1 | grep -i error

# Time query execution
time nexus query "MATCH (n) RETURN count(n)"
```
