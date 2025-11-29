# Changelog

All notable changes to the n8n-nodes-nexus project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.12.0] - 2025-11-28

### Added
- Multi-database support operations:
  - List databases operation
  - Create database operation
  - Get database info operation
  - Drop database operation
  - Switch database operation
- Database selector in node configuration
- Full data isolation between databases

### Changed
- Updated to work with Nexus Server v0.12.0
- Enhanced error handling for database operations

## [0.11.0] - 2024-01-15

### Added

#### Core Operations
- Execute Cypher query operation with full parameter binding support
- Complete CRUD operations for nodes (Create, Read, Update, Delete)
- Complete CRUD operations for relationships
- Find nodes by label and properties operation
- Batch create nodes operation with configurable batch size
- Batch create relationships operation
- List all node labels operation
- List all relationship types operation
- Get complete database schema operation
- Shortest path algorithm between two nodes

#### Authentication
- API key authentication credential type
- User/password authentication credential type
- Automatic token management for authenticated requests
- Secure credential storage via n8n credential system

#### Features
- Dynamic property fields for node/relationship operations
- Label selection UI with validation
- Relationship type selection
- Source/target node selection for relationships
- Comprehensive error handling with detailed messages
- Connection timeout configuration
- Automatic retry logic for failed requests
- Request/response logging for debugging

#### Testing
- 24 comprehensive unit tests
- Client wrapper tests
- Credential validation tests
- Operation implementation tests
- Error handling tests

#### Documentation
- Complete README with usage examples
- 3 workflow examples (Data Import, Graph Analysis, Social Network)
- API documentation with JSDoc comments
- Troubleshooting guide
- Installation instructions

#### Development
- TypeScript configuration with strict mode
- ESLint configuration for code quality
- Prettier configuration for code formatting
- Vitest test framework setup
- Build scripts for development and production

### Technical Details
- Built for n8n v1.x API
- Minimum Node.js version: 18.x
- TypeScript strict mode enabled
- Full type safety with TypeScript interfaces
- Modular code organization
- Comprehensive error handling

## [Unreleased]

### Planned Features
- Vector search operation using KNN index
- Geospatial query operations
- Advanced graph algorithms (PageRank, Betweenness Centrality)
- Transaction support for multi-step operations
- Query result streaming for large datasets
- Custom result transformation options
- Graph export/import operations
- Performance monitoring metrics

### Under Consideration
- GraphQL query support
- Visual query builder
- Schema migration tools
- Backup and restore operations
- Multi-database support
- Read replica configuration

## Version History

### Version Numbering

This package follows semantic versioning:
- **Major version** (X.0.0): Breaking changes, major new features
- **Minor version** (0.X.0): New features, backward compatible
- **Patch version** (0.0.X): Bug fixes, backward compatible

### Release Notes Format

Each version includes:
- **Added**: New features and capabilities
- **Changed**: Changes to existing functionality
- **Deprecated**: Features marked for removal
- **Removed**: Removed features
- **Fixed**: Bug fixes
- **Security**: Security vulnerability fixes

## Upgrade Guide

### From Pre-release to 0.11.0

This is the initial public release. No migration needed.

## Support

- **Issues**: Report bugs at [GitHub Issues](https://github.com/hivellm/nexus/issues)
- **Discussions**: Join conversations in GitHub Discussions
- **Documentation**: [README](README.md)

## Links

- [Repository](https://github.com/hivellm/nexus)
- [npm Package](https://www.npmjs.com/package/@hivellm/n8n-nodes-nexus)
- [n8n Documentation](https://docs.n8n.io/)
- [Nexus Documentation](https://github.com/hivellm/nexus)
