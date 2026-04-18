# Changelog

All notable changes to the Nexus CLI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.12.0] - 2025-11-28

### Added

#### Enhanced Interactive Mode
- **Tab Completion** - Press Tab to auto-complete Cypher keywords, functions, and CLI commands
  - 120+ Cypher keywords supported (MATCH, CREATE, WHERE, RETURN, etc.)
  - 50+ Cypher functions (count, sum, avg, substring, etc.)
  - Special commands (`:quit`, `:history`, `:help`, etc.)
  - Smart completion that matches prefix case-insensitively
- **Improved Line Editing** - Powered by rustyline library
  - Multi-line editing with visual continuation prompt
  - Command history navigation with ↑/↓ arrow keys
  - Ctrl+C to cancel current query (instead of exiting)
  - Ctrl+D or `:quit` to exit
  - Persistent history across sessions
  - Bracket matching highlighting

#### Shell Completion Scripts
- **`nexus completion <shell>`** - Generate shell completion scripts
  - Bash completion support
  - Zsh completion support
  - Fish completion support
  - PowerShell completion support
  - Elvish completion support
- Easy installation: `nexus completion bash > /etc/bash_completion.d/nexus`
- Enables auto-completion of all nexus commands and options in your shell

### Changed
- Interactive mode now uses rustyline for better user experience
- Tab key now triggers keyword completion instead of inserting a tab character
- `:help` command updated to mention Tab completion

### Technical Details
- Added `rustyline 14.0` dependency for advanced line editing
- Added `rustyline-derive 0.10` for helper trait derivation
- Added `clap_complete 4.5` for shell completion generation
- New `cypher_helper` module with CypherHelper, CypherCompleter, and CypherValidator
- New `completion` command module for shell completion generation

## [0.11.0] - 2024-11-28

### Added

#### Core Features
- **Query Commands**
  - `nexus query "<cypher>"` - Execute Cypher queries
  - `nexus query --file <file>` - Execute queries from file
  - `nexus query --interactive` - Interactive REPL mode
  - Query result formatting: table (default), JSON, CSV
  - Parameter binding support via `--params`
  - Query history with `:history` and `:!N` commands
  - Result pagination with `--limit` and `--skip`
  - Result filtering with `--filter column=value`
  - Result sorting with `--sort column` (prefix `-` for descending)

- **Batch Operations**
  - `nexus query --batch <file>` - Execute batch script file
  - `--progress` flag for progress reporting
  - `--dry-run` flag for validation without execution
  - `--stop-on-error` flag to halt on first error
  - Multi-line query support in batch files

- **Database Management**
  - `nexus db ping` - Test server connectivity
  - `nexus db info` - Show database statistics
  - `nexus db clear` - Clear all data
  - `nexus db list` - List all databases
  - `nexus db create <name>` - Create new database
  - `nexus db switch <name>` - Switch to database
  - `nexus db drop <name>` - Drop database

- **User Management**
  - `nexus user list` - List all users
  - `nexus user create <username>` - Create user
  - `nexus user get <username>` - Get user information
  - `nexus user update <username>` - Update password/roles
  - `nexus user passwd [username]` - Interactive password change
  - `nexus user delete <username>` - Delete user
  - Role-based access control support

- **API Key Management**
  - `nexus key list` - List all API keys
  - `nexus key create <name>` - Create API key
  - `nexus key get <id>` - Get key information
  - `nexus key rotate <id>` - Rotate key (revoke and create new)
  - `nexus key revoke <id>` - Revoke API key
  - Support for permissions, rate limits, and expiration

- **Schema Management**
  - `nexus schema labels list` - List node labels
  - `nexus schema labels create <name>` - Create label
  - `nexus schema labels delete <name>` - Delete nodes with label
  - `nexus schema types list` - List relationship types
  - `nexus schema types create <name>` - Create relationship type
  - `nexus schema types delete <name>` - Delete relationships of type
  - `nexus schema indexes list` - List indexes
  - `nexus schema indexes create` - Create index
  - `nexus schema indexes delete <name>` - Delete index

- **Data Import/Export**
  - `nexus data import <file>` - Import data (JSON, CSV, Cypher)
  - `nexus data export <file>` - Export data
  - `nexus data backup <dest>` - Create backup
  - `nexus data backup --compress` - Create compressed backup
  - `nexus data restore <src>` - Restore from backup
  - `nexus data backups [--dir]` - List available backups
  - Progress bars for long operations
  - Batch size configuration

- **Admin Commands**
  - `nexus admin status` - Show server status
  - `nexus admin health` - Health check
  - `nexus admin stats` - Show statistics

- **Configuration Management**
  - `nexus config init` - Initialize configuration
  - `nexus config show` - Display current configuration
  - `nexus config get <key>` - Get configuration value
  - `nexus config set <key> <value>` - Set configuration value
  - `nexus config profile` - Manage connection profiles
  - `nexus config path` - Show config file location
  - Support for multiple connection profiles
  - Environment variable overrides

#### Global Options
- `--config <path>` - Custom config file path
- `--url <url>` - Server URL override
- `--api-key <key>` - API key authentication
- `--username <user>` - Username for authentication
- `--password <pass>` - Password for authentication
- `--profile <name>` - Use named connection profile
- `-v, --verbose` - Verbose output
- `--debug` - Debug logging
- `--json` - JSON output format
- `--csv` - CSV output format

#### Interactive REPL Features
- Multi-line query support
- Query history persistence
- Special commands: `:help`, `:history`, `:!N`, `:clear`, `:quit`
- Colored output for better readability
- Auto-completion hints

#### User Experience
- Colored terminal output
- Progress bars for long operations
- Spinners for background tasks
- Clear error messages
- Exit codes for scripting
- Table formatting with borders
- Automatic column width adjustment

#### Testing & Quality
- 36 unit tests (all passing)
- 22 CLI integration tests
- End-to-end testing with real server
- Comprehensive error handling
- Input validation

#### Documentation
- Comprehensive README.md
- Man pages (nexus.1, nexus-query.1, nexus-db.1, nexus-user.1, nexus-key.1)
- Best practices guide
- Installation scripts (Linux, macOS, Windows)
- Usage examples
- Troubleshooting guide

#### Distribution
- GitHub Actions CI/CD pipeline
- Automated binary releases
- Cross-platform builds (Windows, Linux, macOS)
- Installation scripts
- Uninstallation support

### Technical Details
- Built with Rust 1.75+
- Uses clap 4.5 for command parsing
- Uses reqwest for HTTP client
- TOML configuration format
- Colored output via `colored` crate
- Progress bars via `indicatif` crate
- Interactive mode via `rustyline` crate

### Known Issues
- Tab completion not yet implemented (planned for future release)
- Syntax highlighting in REPL not yet implemented
- Some package manager distributions pending (Homebrew, apt, rpm, Chocolatey)

### Breaking Changes
None - this is the initial release

## [Unreleased]

### Planned Features
- Tab completion for commands and Cypher keywords
- Syntax highlighting in interactive mode
- Query templates and snippets
- Auto-update functionality
- Package manager distributions (Homebrew, apt, rpm, Chocolatey, Scoop)
- Binary signing for security
- Shell completion scripts (bash, zsh, fish, PowerShell)
- Advanced query profiling and analysis
- Visual query result rendering (ASCII graphs)
- Export to additional formats (GraphML, GEXF)
- Connection pooling for batch operations
- Transaction management commands
- Database migration tools
- Performance monitoring dashboard
- Plugin system for extensions

## Version History

### Version Numbering
This project follows semantic versioning:
- **Major version** (X.0.0): Breaking changes
- **Minor version** (0.X.0): New features, backward compatible
- **Patch version** (0.0.X): Bug fixes, backward compatible

## Migration Guides

### From Pre-release to 0.11.0
This is the initial public release. No migration needed.

## Support

- **Issues**: Report bugs at [GitHub Issues](https://github.com/hivellm/nexus/issues)
- **Discussions**: Join conversations in GitHub Discussions
- **Documentation**: [README](README.md) | [Best Practices](docs/BEST_PRACTICES.md)

## Contributors

Thanks to everyone who contributed to this release!

## Links

- [Repository](https://github.com/hivellm/nexus)
- [Releases](https://github.com/hivellm/nexus/releases)
- [Documentation](https://github.com/hivellm/nexus/tree/main/nexus-cli)
- [Nexus Server](https://github.com/hivellm/nexus)
