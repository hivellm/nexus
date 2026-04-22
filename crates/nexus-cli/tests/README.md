# Nexus CLI Tests

This directory contains tests for the Nexus CLI tool.

## Test Types

### 1. Unit Tests (`cli_tests.rs`, `config_tests.rs`)

These tests verify CLI command-line parsing, help text, and basic functionality without requiring a server.

**Run with:**
```bash
cargo test -p nexus-cli
```

### 2. Integration Tests (`multi_database_cli_integration_test.rs`)

These tests verify end-to-end CLI functionality by running commands against a real Nexus server.

**⚠️ Important:** These tests are marked as `#[ignore]` because they:
- Require starting a real Nexus server
- Take longer to run
- May conflict with other running instances

**Run with:**
```bash
# Run all ignored tests
cargo test -p nexus-cli --test multi_database_cli_integration_test -- --ignored

# Run a specific test
cargo test -p nexus-cli --test multi_database_cli_integration_test test_cli_db_list -- --ignored
```

## Multi-Database Integration Tests

The `multi_database_cli_integration_test.rs` file tests the following CLI commands:

### Basic Commands
- `nexus db list` - List all databases
- `nexus db info` - Show database statistics
- `nexus db ping` - Test server connectivity

### Database Lifecycle
- `nexus db create <name>` - Create a new database
- `nexus db switch <name>` - Switch to a different database
- `nexus db drop <name>` - Drop a database

### Test Coverage

1. **test_cli_db_list** - Verifies listing databases
2. **test_cli_db_create_and_drop** - Tests creating and dropping databases
3. **test_cli_db_switch** - Tests switching between databases
4. **test_cli_db_info** - Tests database information display
5. **test_cli_full_database_lifecycle** - Comprehensive end-to-end test
6. **test_cli_db_ping** - Tests server connectivity
7. **test_cli_db_create_duplicate_fails** - Tests error handling
8. **test_cli_db_drop_nonexistent_fails** - Tests error handling

## Test Server Management

The tests automatically:
- Start a Nexus server on a unique port (7688-7695)
- Wait for the server to be ready
- Clean up the server process on test completion
- Use isolated data directories per test

## Manual Testing

For manual testing with a running server:

```bash
# Start the server
cargo run -p nexus-server

# In another terminal, run CLI commands
cargo run -p nexus-cli -- db list
cargo run -p nexus-cli -- db create mydb
cargo run -p nexus-cli -- db switch mydb
cargo run -p nexus-cli -- db info
cargo run -p nexus-cli -- db drop mydb --force
```

## Troubleshooting

### Tests hang or timeout
- Ensure no other Nexus servers are running on the test ports
- Check that the server binary can be built successfully
- Increase the startup timeout in the test code if needed

### Tests fail with "not supported"
- This is expected if multi-database support is not fully implemented
- The tests gracefully handle this and log appropriate messages

### Port conflicts
- Each test uses a unique port (7688-7695)
- If you see port conflicts, check for orphaned server processes

## Contributing

When adding new CLI commands, please:
1. Add unit tests for help text and argument parsing
2. Add integration tests for end-to-end functionality
3. Mark integration tests with `#[ignore]` if they require a server
4. Use unique ports for each integration test
5. Clean up resources properly in test cleanup
