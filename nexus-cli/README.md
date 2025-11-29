# Nexus CLI

Command-line interface for Nexus graph database.

## Installation

### From Source

```bash
cargo install --path nexus-cli
```

### Pre-built Binary

Download the latest release from the [releases page](https://github.com/hivellm/nexus/releases).

## Quick Start

```bash
# Ping the server
nexus --url http://localhost:3000 db ping

# Execute a query
nexus --url http://localhost:3000 query "MATCH (n) RETURN n LIMIT 5"

# Interactive mode
nexus --url http://localhost:3000 query --interactive
```

## Configuration

### Configuration File

Create a configuration file at `~/.config/nexus/config.toml` (Linux/macOS) or `%APPDATA%\nexus\config.toml` (Windows):

```toml
url = "http://localhost:3000"
username = "root"
password = "your-password"

[profiles.production]
url = "http://production:3000"
api_key = "your-api-key"

[profiles.development]
url = "http://localhost:3000"
username = "dev"
password = "dev-password"
```

### Environment Variables

- `NEXUS_URL` - Server URL
- `NEXUS_API_KEY` - API key for authentication
- `NEXUS_USERNAME` - Username for authentication
- `NEXUS_PASSWORD` - Password for authentication
- `NEXUS_PROFILE` - Default connection profile
- `NEXUS_CONFIG` - Path to configuration file

### Initialize Configuration

```bash
nexus config init
```

## Commands

### Query Commands

```bash
# Execute a Cypher query
nexus query "MATCH (n:Person) RETURN n.name"

# Execute from file
nexus query --file queries.cypher

# With parameters
nexus query "MATCH (n) WHERE n.name = \$name RETURN n" --params '{"name": "Alice"}'

# Interactive REPL
nexus query --interactive

# Output formats
nexus --json query "MATCH (n) RETURN n LIMIT 5"
nexus --csv query "MATCH (n) RETURN n.name, n.age"
```

### Database Commands

```bash
# Ping server
nexus db ping

# Get database info
nexus db info

# Clear all data (use with caution!)
nexus db clear
```

### User Management

```bash
# List users
nexus user list

# Create user
nexus user create myuser --password secret --roles admin,reader

# Get user info
nexus user get myuser

# Delete user
nexus user delete myuser
```

### API Key Management

```bash
# List API keys
nexus key list

# Create API key
nexus key create mykey --permissions read,write

# Get key info
nexus key get <key-id>

# Revoke key
nexus key revoke <key-id>
```

### Schema Commands

```bash
# List labels
nexus schema labels list

# List relationship types
nexus schema types list

# List indexes
nexus schema indexes list
```

### Data Import/Export

```bash
# Export data
nexus data export backup.json --format json
nexus data export backup.csv --format csv
nexus data export backup.cypher --format cypher

# Import data
nexus data import data.json --format json
nexus data import data.csv --format csv --batch-size 1000
```

### Admin Commands

```bash
# Server status
nexus admin status

# Health check
nexus admin health

# Database statistics
nexus admin stats
```

### Configuration Commands

```bash
# Show current config
nexus config show

# Show config file path
nexus config path

# Set a value
nexus config set url http://myserver:3000

# Get a value
nexus config get url

# Manage profiles
nexus config profile list
nexus config profile add prod --url http://prod:3000 --api-key mykey
nexus config profile remove prod
nexus config profile default prod
```

## Global Options

| Option | Environment | Description |
|--------|-------------|-------------|
| `--url <URL>` | `NEXUS_URL` | Server URL |
| `--api-key <KEY>` | `NEXUS_API_KEY` | API key |
| `--username <USER>` | `NEXUS_USERNAME` | Username |
| `--password <PASS>` | `NEXUS_PASSWORD` | Password |
| `--profile <NAME>` | `NEXUS_PROFILE` | Connection profile |
| `--config <PATH>` | `NEXUS_CONFIG` | Config file path |
| `--json` | - | JSON output format |
| `--csv` | - | CSV output format |
| `-v, --verbose` | - | Verbose output |
| `--debug` | - | Debug output |

## Examples

### Create and Query Nodes

```bash
# Create nodes
nexus query "CREATE (a:Person {name: 'Alice', age: 30})"
nexus query "CREATE (b:Person {name: 'Bob', age: 25})"
nexus query "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})"

# Query nodes
nexus query "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.age"

# Query relationships
nexus query "MATCH (a)-[r:KNOWS]->(b) RETURN a.name, type(r), b.name"
```

### Using Profiles

```bash
# Use a specific profile
nexus --profile production query "MATCH (n) RETURN count(n)"

# Set default profile
nexus config profile default production
```

### Scripting

```bash
# Export query results to file
nexus --json query "MATCH (n) RETURN n" > nodes.json

# Pipe queries
echo "MATCH (n) RETURN count(n)" | nexus query --file -

# Check server health in scripts
if nexus db ping; then
    echo "Server is up"
else
    echo "Server is down"
    exit 1
fi
```

## Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1 | General error |
| 2 | Connection error |
| 3 | Authentication error |
| 4 | Query error |

## Troubleshooting

### Connection Issues

```bash
# Check if server is reachable
nexus --url http://localhost:3000 db ping

# Verbose output for debugging
nexus --verbose --url http://localhost:3000 db info
```

### Authentication Issues

```bash
# Test with explicit credentials
nexus --url http://localhost:3000 --username root --password secret db info

# Test with API key
nexus --url http://localhost:3000 --api-key your-key db info
```

## License

Apache-2.0
