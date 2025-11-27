# Proposal: Nexus Command Line Interface (CLI)

## Why

A command-line interface is essential for database administration, automation, scripting, and DevOps workflows. Creating an official Nexus CLI will enable administrators and developers to manage databases, users, API keys, execute queries, and perform administrative tasks from the command line. This will significantly improve operational efficiency, enable automation, and make Nexus more accessible for system administrators and developers who prefer command-line tools.

## Purpose

Create an official Command Line Interface (CLI) for Nexus graph database to enable command-line operations for database management, user administration, API key management, query execution, and administrative tasks. This will provide a powerful tool for automation, scripting, and operational workflows.

## Context

Currently, Nexus provides REST APIs that can be consumed via HTTP clients, but there's no official CLI tool. Administrators and developers must use curl or custom scripts for command-line operations. By providing an official CLI, we can:

- Enable database administration from command line
- Support automation and scripting workflows
- Provide user-friendly command interface
- Enable batch operations
- Support configuration management
- Enable integration with shell scripts and CI/CD pipelines

## Scope

This proposal covers:

1. **CLI Application**
   - Command parsing and execution
   - Configuration management
   - Connection management
   - Output formatting
   - Error handling

2. **Core Commands**
   - Database management (create, list, delete, switch)
   - User management (create, list, update, delete)
   - API key management (create, list, revoke)
   - Query execution (Cypher queries)
   - Schema operations (labels, relationship types, indexes)
   - Data operations (import, export)

3. **Advanced Features**
   - Interactive mode (REPL)
   - Batch mode (script execution)
   - Output formats (JSON, table, CSV)
   - Configuration file support
   - Connection profiles

4. **Distribution**
   - Binary releases for major platforms
   - Package managers (Homebrew, apt, etc.)
   - Installation scripts
   - Documentation

## Requirements

### Core CLI Features

The CLI MUST provide:

1. **Command Structure**
   - Subcommand-based architecture
   - Help system (`--help` for all commands)
   - Version information (`--version`)
   - Configuration file support (`--config`)
   - Verbose/debug mode (`--verbose`, `--debug`)

2. **Connection Management**
   - Connection configuration (URL, credentials)
   - Connection profiles
   - Connection testing
   - Secure credential storage

3. **Output Formatting**
   - Table format (default)
   - JSON format (`--json`)
   - CSV format (`--csv`)
   - Custom formatting options

4. **Error Handling**
   - Clear error messages
   - Exit codes
   - Error recovery suggestions
   - Verbose error details

### Command Categories

#### Database Commands

```bash
nexus db create <name>
nexus db list
nexus db delete <name>
nexus db switch <name>
nexus db info <name>
nexus db stats <name>
```

#### User Commands

```bash
nexus user create <username> [--password] [--roles]
nexus user list
nexus user get <username>
nexus user update <username> [--password] [--roles]
nexus user delete <username>
```

#### API Key Commands

```bash
nexus key create <name> [--permissions] [--rate-limit]
nexus key list
nexus key get <id>
nexus key revoke <id>
nexus key rotate <id>
```

#### Query Commands

```bash
nexus query "<cypher query>" [--params]
nexus query --file <file> [--params]
nexus query --interactive
```

#### Schema Commands

```bash
nexus schema labels [list|create|delete]
nexus schema types [list|create|delete]
nexus schema indexes [list|create|delete]
```

#### Data Commands

```bash
nexus data import <file> [--format] [--batch-size]
nexus data export <file> [--format] [--query]
nexus data backup <destination>
nexus data restore <source>
```

#### Admin Commands

```bash
nexus admin status
nexus admin health
nexus admin stats
nexus admin config [get|set]
```

## Implementation Strategy

### Phase 1: Core CLI Framework
- Set up CLI project structure
- Implement command parsing (clap/structopt)
- Add configuration management
- Implement connection handling

### Phase 2: Basic Commands
- Implement database commands
- Implement query execution
- Add output formatting
- Add error handling

### Phase 3: User & API Key Management
- Implement user commands
- Implement API key commands
- Add authentication handling

### Phase 4: Advanced Features
- Add interactive mode (REPL)
- Add batch mode
- Add import/export
- Add schema commands

### Phase 5: Testing & Documentation
- Comprehensive test suite
- Integration tests
- Documentation and examples
- Installation guides

### Phase 6: Distribution
- Build binaries for platforms
- Create installation packages
- Set up package manager distribution
- Create installation scripts

## Success Criteria

- CLI published and available for download
- All core commands functional
- ≥90% test coverage
- Comprehensive documentation
- Installation packages for major platforms
- Interactive mode working
- Batch mode working
- CI/CD pipeline for automated builds

## Dependencies

- Rust (for CLI implementation)
- Nexus REST API (already available)
- Nexus authentication (already implemented)
- clap or structopt (for command parsing)
- serde (for JSON handling)

## Use Cases

1. **Database Administration**
   - Create and manage databases
   - Monitor database status
   - Perform administrative tasks

2. **User Management**
   - Create and manage users
   - Assign roles and permissions
   - Manage API keys

3. **Query Execution**
   - Execute Cypher queries from command line
   - Run query scripts
   - Interactive query execution

4. **Automation & Scripting**
   - Integrate with shell scripts
   - CI/CD pipeline integration
   - Automated backups
   - Scheduled tasks

5. **Data Management**
   - Import/export data
   - Backup and restore
   - Data migration

## Future Enhancements

- Tab completion (bash, zsh, fish)
- Command aliases
- Query history
- Query templates
- Graph visualization output
- Performance profiling
- Query plan visualization
- Multi-database operations
- Cluster management commands
