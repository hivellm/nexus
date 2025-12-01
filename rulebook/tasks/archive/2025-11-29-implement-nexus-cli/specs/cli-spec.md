# Nexus CLI Specification

## Command Structure

### Global Options

All commands support these global options:

- `--config <file>` - Path to configuration file
- `--url <url>` - Nexus server URL
- `--api-key <key>` - API key for authentication
- `--username <user>` - Username for authentication
- `--password <pass>` - Password for authentication
- `--profile <name>` - Connection profile name
- `--verbose` - Verbose output
- `--debug` - Debug output
- `--json` - JSON output format
- `--csv` - CSV output format
- `--help` - Show help
- `--version` - Show version

## Command Specifications

### Database Commands

#### `nexus db create <name>`

Create a new database.

**Options**:
- `--description <text>` - Database description

**Example**:
```bash
nexus db create mydb --description "My database"
```

**Output**:
```
Database 'mydb' created successfully
```

#### `nexus db list`

List all databases.

**Options**:
- `--format <format>` - Output format (table, json, csv)

**Example**:
```bash
nexus db list --format table
```

**Output**:
```
NAME    SIZE    NODES    RELATIONSHIPS
mydb    1.2GB   100K     500K
testdb  500MB   50K      200K
```

#### `nexus db delete <name>`

Delete a database.

**Options**:
- `--force` - Skip confirmation

**Example**:
```bash
nexus db delete mydb --force
```

#### `nexus db switch <name>`

Switch active database.

**Example**:
```bash
nexus db switch mydb
```

#### `nexus db info <name>`

Show database information.

**Example**:
```bash
nexus db info mydb
```

**Output**:
```
Database: mydb
Size: 1.2GB
Nodes: 100,000
Relationships: 500,000
Labels: 10
Relationship Types: 5
Created: 2025-01-01 10:00:00
```

#### `nexus db stats <name>`

Show database statistics.

**Example**:
```bash
nexus db stats mydb
```

### User Commands

#### `nexus user create <username>`

Create a new user.

**Options**:
- `--password <pass>` - User password (prompt if not provided)
- `--roles <roles>` - Comma-separated roles
- `--email <email>` - User email

**Example**:
```bash
nexus user create alice --password secret123 --roles admin,user
```

#### `nexus user list`

List all users.

**Options**:
- `--format <format>` - Output format

**Example**:
```bash
nexus user list
```

**Output**:
```
USERNAME    ROLES           CREATED
admin       admin           2025-01-01
alice       admin,user      2025-01-02
bob         user            2025-01-03
```

#### `nexus user get <username>`

Get user information.

**Example**:
```bash
nexus user get alice
```

#### `nexus user update <username>`

Update user.

**Options**:
- `--password <pass>` - Update password
- `--roles <roles>` - Update roles
- `--email <email>` - Update email

**Example**:
```bash
nexus user update alice --roles admin
```

#### `nexus user delete <username>`

Delete a user.

**Options**:
- `--force` - Skip confirmation

**Example**:
```bash
nexus user delete alice --force
```

### API Key Commands

#### `nexus key create <name>`

Create a new API key.

**Options**:
- `--permissions <perms>` - Comma-separated permissions (read, write, admin)
- `--rate-limit <limit>` - Rate limit (format: 1000/min,10000/hour)

**Example**:
```bash
nexus key create mykey --permissions read,write --rate-limit 1000/min,10000/hour
```

**Output**:
```
API Key created successfully
ID: key_abc123
Key: nexus_sk_abc123def456... (save this, it won't be shown again)
```

#### `nexus key list`

List all API keys.

**Example**:
```bash
nexus key list
```

**Output**:
```
ID          NAME      PERMISSIONS    RATE LIMIT        CREATED
key_abc123  mykey     read,write     1000/min          2025-01-01
key_def456  prodkey   admin          unlimited         2025-01-02
```

#### `nexus key get <id>`

Get API key information.

**Example**:
```bash
nexus key get key_abc123
```

#### `nexus key revoke <id>`

Revoke an API key.

**Options**:
- `--force` - Skip confirmation

**Example**:
```bash
nexus key revoke key_abc123 --force
```

#### `nexus key rotate <id>`

Rotate an API key (create new, revoke old).

**Example**:
```bash
nexus key rotate key_abc123
```

### Query Commands

#### `nexus query "<cypher>"`

Execute a Cypher query.

**Options**:
- `--params <json>` - Query parameters (JSON)
- `--file <file>` - Read query from file
- `--format <format>` - Output format

**Example**:
```bash
nexus query "MATCH (n:Person) RETURN n.name, n.age LIMIT 10"
```

**Output**:
```
n.name    n.age
Alice      30
Bob        25
Charlie    35
```

#### `nexus query --file <file>`

Execute query from file.

**Example**:
```bash
nexus query --file query.cypher --params '{"limit": 20}'
```

#### `nexus query --interactive`

Start interactive query shell (REPL).

**Example**:
```bash
nexus query --interactive
```

**REPL Features**:
- Query history (up/down arrows)
- Multi-line queries
- Command completion
- Query templates
- Exit with `exit` or `quit`

### Schema Commands

#### `nexus schema labels list`

List all labels.

**Example**:
```bash
nexus schema labels list
```

#### `nexus schema labels create <name>`

Create a label.

**Example**:
```bash
nexus schema labels create Person
```

#### `nexus schema labels delete <name>`

Delete a label.

**Example**:
```bash
nexus schema labels delete Person --force
```

#### `nexus schema types list`

List all relationship types.

**Example**:
```bash
nexus schema types list
```

#### `nexus schema types create <name>`

Create a relationship type.

**Example**:
```bash
nexus schema types create KNOWS
```

#### `nexus schema indexes list`

List all indexes.

**Example**:
```bash
nexus schema indexes list
```

#### `nexus schema indexes create`

Create an index.

**Options**:
- `--type <type>` - Index type (label, property, fulltext)
- `--label <label>` - Label for index
- `--property <prop>` - Property for index

**Example**:
```bash
nexus schema indexes create --type label --label Person
```

### Data Commands

#### `nexus data import <file>`

Import data from file.

**Options**:
- `--format <format>` - File format (json, csv, cypher)
- `--batch-size <size>` - Batch size (default: 1000)

**Example**:
```bash
nexus data import data.json --format json --batch-size 5000
```

#### `nexus data export <file>`

Export data to file.

**Options**:
- `--format <format>` - Output format (json, csv, cypher)
- `--query <query>` - Export query results

**Example**:
```bash
nexus data export output.json --format json --query "MATCH (n) RETURN n"
```

#### `nexus data backup <destination>`

Backup database.

**Options**:
- `--compress` - Compress backup
- `--include-indexes` - Include indexes

**Example**:
```bash
nexus data backup /backup/mydb.tar.gz --compress
```

#### `nexus data restore <source>`

Restore database from backup.

**Options**:
- `--force` - Overwrite existing database

**Example**:
```bash
nexus data restore /backup/mydb.tar.gz --force
```

### Admin Commands

#### `nexus admin status`

Show server status.

**Example**:
```bash
nexus admin status
```

**Output**:
```
Server: Running
Version: 0.11.0
Uptime: 5d 12h 30m
Databases: 3
Connections: 15
```

#### `nexus admin health`

Check server health.

**Example**:
```bash
nexus admin health
```

**Output**:
```
Status: Healthy
Database: OK
Storage: OK
Memory: OK
```

#### `nexus admin stats`

Show server statistics.

**Example**:
```bash
nexus admin stats
```

#### `nexus admin config get <key>`

Get configuration value.

**Example**:
```bash
nexus admin config get server.port
```

#### `nexus admin config set <key> <value>`

Set configuration value.

**Example**:
```bash
nexus admin config set server.port 15475
```

## Configuration File

### Location

- Linux/macOS: `~/.config/nexus/config.toml`
- Windows: `%APPDATA%\nexus\config.toml`

### Format

```toml
[default]
url = "http://localhost:15474"
api_key = "nexus_sk_..."

[profiles.production]
url = "http://nexus.example.com:15474"
api_key = "nexus_sk_prod_..."

[profiles.development]
url = "http://localhost:15474"
username = "admin"
password = "secret"
```

## Exit Codes

- `0` - Success
- `1` - General error
- `2` - Configuration error
- `3` - Connection error
- `4` - Authentication error
- `5` - Query error
- `6` - Validation error

## Error Handling

### Error Format

```json
{
  "error": {
    "type": "QueryError",
    "message": "Syntax error in Cypher query",
    "code": 5,
    "details": {
      "line": 1,
      "column": 10
    }
  }
}
```

### Error Display

- Clear error messages
- Error type indication
- Suggestion for recovery
- Verbose details with `--verbose`

## Testing Requirements

### Unit Tests

- Test command parsing
- Test configuration handling
- Test connection management
- Test error handling
- ≥90% code coverage

### Integration Tests

- Test with real Nexus server
- Test all commands end-to-end
- Test interactive mode
- Test batch mode
- Test error scenarios

### CLI Tests

- Test command-line interface
- Test help system
- Test output formatting
- Test exit codes

