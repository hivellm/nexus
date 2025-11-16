# Nexus Core Tests

## Running Tests

### Standard Tests

Run all standard tests (excluding slow tests):

```bash
cargo test --package nexus-core
```

### Slow Tests

Some tests are marked as slow and are skipped by default. To run them, enable the `slow-tests` feature:

```bash
cargo test --package nexus-core --features slow-tests
```

### Server-to-Server (S2S) Tests

Tests that require the server to be running use the `s2s` feature:

```bash
cargo test --package nexus-core --features s2s
```

### Running Specific Test Files

```bash
# Run only unit tests
cargo test --package nexus-core --lib

# Run a specific test file
cargo test --package nexus-core --test in_operator_tests

# Run slow tests from a specific file
cargo test --package nexus-core --test performance_tests --features slow-tests
```

## Test Categories

### Unit Tests (`--lib`)
- Fast tests that don't require external services
- Located in `src/` modules
- Run by default

### Integration Tests (`--test`)
- Tests in `tests/` directory
- May require setup or external services
- Run by default (except slow tests)

### Slow Tests (`--features slow-tests`)
- Tests that take a long time to run
- Memory management tests
- Performance benchmarks
- Skipped by default to speed up CI/CD

### S2S Tests (`--features s2s`)
- Server-to-server integration tests
- Require Nexus server to be running
- Located in `tests/*_s2s_test.rs`

## Test Timeouts

Tests that may hang or take too long should use timeouts. For async tests, use `tokio::time::timeout`:

```rust
#[tokio::test]
async fn test_with_timeout() {
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        async {
            // Test code here
        }
    ).await;
    
    assert!(result.is_ok(), "Test timed out after 30 seconds");
}
```

## Adding New Tests

### Standard Test

```rust
#[test]
fn my_test() {
    // Test code
}
```

### Slow Test

```rust
#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "Slow test - enable with --features slow-tests")]
fn my_slow_test() {
    // Slow test code
}
```

### Async Test with Timeout

```rust
#[tokio::test]
async fn my_async_test() {
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        async {
            // Test code
        }
    ).await;
    
    assert!(result.is_ok());
}
```

## CI/CD Configuration

In CI/CD pipelines, run tests in stages:

1. **Fast tests** (default): `cargo test --package nexus-core`
2. **Slow tests** (optional, nightly): `cargo test --package nexus-core --features slow-tests`
3. **S2S tests** (requires server): `cargo test --package nexus-core --features s2s`

