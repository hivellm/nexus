---
title: Troubleshooting
module: operations
id: troubleshooting
order: 5
description: Common problems and solutions
tags: [troubleshooting, problems, solutions, debugging]
---

# Troubleshooting

Common problems and solutions for Nexus.

## Common Issues

### Server Won't Start

**Problem:** Server fails to start or crashes immediately.

**Solutions:**

1. **Check Port Availability:**
   ```bash
   # Linux/macOS
   lsof -i :15474
   
   # Windows
   netstat -ano | findstr :15474
   ```

2. **Check Logs:**
   ```bash
   # Linux
   sudo journalctl -u nexus -n 100
   
   # Windows
   Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100
   ```

3. **Check Permissions:**
   ```bash
   # Ensure data directory is writable
   chmod -R 755 /path/to/data
   ```

4. **Check Disk Space:**
   ```bash
   df -h
   ```

### Query Timeout

**Problem:** Queries timeout or take too long.

**Solutions:**

1. **Increase Timeout:**
   ```cypher
   // Use timeout parameter in API
   {
     "query": "MATCH (n) RETURN n",
     "timeout_ms": 30000
   }
   ```

2. **Optimize Query:**
   - Add LIMIT clause
   - Use indexes
   - Filter early with WHERE
   - Avoid Cartesian products

3. **Check Query Plan:**
   ```cypher
   EXPLAIN MATCH (n) RETURN n
   ```

### Memory Issues

**Problem:** High memory usage or out of memory errors.

**Solutions:**

1. **Check Memory Usage:**
   ```bash
   curl http://localhost:15474/stats | jq '.memory_usage_mb'
   ```

2. **Reduce Cache Size:**
   ```yaml
   # config.yml
   cache:
     max_size_mb: 1024
   ```

3. **Limit Query Results:**
   ```cypher
   MATCH (n) RETURN n LIMIT 1000
   ```

### Authentication Errors

**Problem:** Authentication fails or permissions denied.

**Solutions:**

1. **Check API Key:**
   ```bash
   # Verify key format
   echo $NEXUS_API_KEY
   ```

2. **Check Permissions:**
   ```cypher
   SHOW USER current
   ```

3. **Reset Root Password:**
   ```bash
   export NEXUS_ROOT_PASSWORD="new_password"
   ```

### Database Not Found

**Problem:** Database doesn't exist or can't be accessed.

**Solutions:**

1. **List Databases:**
   ```cypher
   SHOW DATABASES
   ```

2. **Create Database:**
   ```cypher
   CREATE DATABASE mydb
   ```

3. **Switch Database:**
   ```cypher
   :USE mydb
   ```

### Vector Dimension Mismatch

**Problem:** Vector operations fail with dimension errors.

**Solutions:**

1. **Check Vector Dimensions:**
   ```cypher
   MATCH (n:Person)
   WHERE n.vector IS NOT NULL
   RETURN DISTINCT size(n.vector) AS dimension
   ```

2. **Normalize Vectors:**
   - Ensure all vectors have the same dimension
   - Use consistent embedding models

3. **Recreate Index:**
   ```cypher
   DROP INDEX ON :Person(vector)
   CREATE INDEX ON :Person(vector)
   ```

## Error Messages

### SyntaxError

**Message:** `Invalid Cypher syntax`

**Solution:** Check query syntax, verify all clauses are correct.

### AuthenticationError

**Message:** `Authentication failed`

**Solution:** Verify API key or JWT token is valid and has correct permissions.

### NotFoundError

**Message:** `Resource not found`

**Solution:** Check if database, node, or relationship exists.

### ValidationError

**Message:** `Invalid input`

**Solution:** Verify input parameters match expected format.

## Debugging Tips

### Enable Debug Logging

```bash
export RUST_LOG=debug
./nexus-server
```

### Check Query Execution

```cypher
// Use EXPLAIN to see query plan
EXPLAIN MATCH (n:Person) RETURN n

// Use PROFILE to see execution stats
PROFILE MATCH (n:Person) RETURN n
```

### Monitor Performance

```bash
# Check statistics
curl http://localhost:15474/stats

# Check health
curl http://localhost:15474/health
```

## Performance Issues

### Slow Queries

1. **Add Indexes:**
   ```cypher
   CREATE INDEX ON :Person(name)
   CREATE INDEX ON :Person(age)
   ```

2. **Use LIMIT:**
   ```cypher
   MATCH (n) RETURN n LIMIT 100
   ```

3. **Filter Early:**
   ```cypher
   MATCH (n:Person)
   WHERE n.age > 25
   RETURN n
   ```

### High Memory Usage

1. **Reduce Cache:**
   ```yaml
   cache:
     max_size_mb: 512
   ```

2. **Limit Connections:**
   ```yaml
   server:
     max_connections: 100
   ```

## Getting Help

1. **Check Logs:** Review server logs for detailed error messages
2. **Check Documentation:** Review relevant guides
3. **Check GitHub Issues:** Search for similar problems
4. **Contact Support:** Reach out to the development team

## Related Topics

- [Service Management](./SERVICE_MANAGEMENT.md) - Managing services
- [Monitoring](./MONITORING.md) - Health checks and metrics
- [Configuration Guide](../configuration/CONFIGURATION.md) - Server configuration

