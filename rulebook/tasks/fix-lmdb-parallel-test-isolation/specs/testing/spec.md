# Spec: Testing Infrastructure for LMDB Parallel Isolation

## ADDED Requirements

### Requirement: Test Harness Module

The system SHALL provide a centralized testing module at `nexus-core/src/testing/mod.rs`.

#### Scenario: Test Context Creation
Given a test needs an isolated executor
When the test calls `TestContext::new()`
Then the system MUST create a unique temporary directory
And the system MUST ensure the directory exists before returning
And the system MUST register the context for cleanup

#### Scenario: Automatic Cleanup
Given a TestContext is dropped
When the Rust drop trait is invoked
Then the system MUST clean up all allocated resources
And the system MUST remove the temporary directory
And the system MUST release any LMDB locks

### Requirement: Resource Pool

The system SHALL implement a resource pool for LMDB environments.

#### Scenario: Environment Reuse
Given multiple tests need LMDB access
When tests request an LMDB environment
Then the system SHOULD reuse environments from the pool
And the system MUST NOT exceed LMDB's environment limit

#### Scenario: Pool Exhaustion
Given the resource pool is exhausted
When a test requests a new environment
Then the system MUST wait for an available environment
And the system MUST NOT create a new environment beyond limits

### Requirement: Test Isolation Levels

The system SHALL support configurable test isolation.

#### Scenario: Serial Test Execution
Given a test requires exclusive database access
When the test is marked with `#[serial]`
Then the system MUST execute the test serially
And no other tests SHALL run concurrently

#### Scenario: Parallel Test Execution
Given a test uses only local resources
When the test is marked as parallelizable
Then the system MAY execute it concurrently with other parallel tests
And each test MUST have its own isolated resources

### Requirement: Standardized Test Helpers

The system SHALL provide standardized test helper functions.

#### Scenario: Create Test Executor
Given a test needs an Executor instance
When calling `testing::create_test_executor()`
Then the system MUST return a valid Executor
And the system MUST guarantee the underlying directory exists
And the function signature MUST be `fn create_test_executor() -> (Executor, TestContext)`

#### Scenario: Setup Test Engine
Given a test needs an Engine instance
When calling `testing::setup_test_engine()`
Then the system MUST return a valid Engine
And the system MUST guarantee all paths exist
And the function signature MUST be `fn setup_test_engine() -> Result<(Engine, TestContext), Error>`

### Requirement: Directory Existence Guarantee

The system SHALL guarantee directory existence before component initialization.

#### Scenario: TempDir with Guarantee
Given `TempDir::new()` is called
When the directory handle is returned
Then `std::fs::create_dir_all(dir.path())` MUST be called
And the call MUST succeed before any component uses the path

#### Scenario: Nested Directory Creation
Given a component needs subdirectories
When the component path is derived from TempDir
Then all parent directories MUST exist
And the system MUST create them if they don't exist

### Requirement: Thread Safety

The system SHALL be thread-safe for parallel test execution.

#### Scenario: Concurrent Access
Given multiple tests run in parallel
When they access the resource pool
Then the system MUST use appropriate synchronization primitives
And there SHALL be no data races

#### Scenario: Thread-Local State
Given some state should not be shared between tests
When using thread-local storage
Then each thread MUST have its own copy
And cleanup MUST happen on thread exit

## MODIFIED Requirements

### Requirement: Test File Structure

Individual test files SHALL NOT define their own test helper functions.

#### Scenario: Import from Testing Module
Given a test file needs test helpers
When the test file is written
Then it MUST import from `nexus_core::testing`
And it MUST NOT define local `create_test_executor()` functions

## Technical Specifications

### Dependencies

```toml
[dev-dependencies]
serial_test = "3.0"
once_cell = "1.18"
```

### Module Structure

```rust
// nexus-core/src/testing/mod.rs
#[cfg(test)]
pub mod testing {
    mod context;
    mod executor;
    mod engine;
    mod pool;
    mod fixtures;
    
    pub use context::TestContext;
    pub use executor::create_test_executor;
    pub use engine::setup_test_engine;
    pub use pool::ResourcePool;
    pub use fixtures::*;
}
```

### TestContext Interface

```rust
pub struct TestContext {
    temp_dir: TempDir,
    resources: Vec<Box<dyn Any>>,
}

impl TestContext {
    pub fn new() -> Self;
    pub fn path(&self) -> &Path;
    pub fn register<T: Any>(&mut self, resource: T);
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Cleanup resources in reverse order
    }
}
```

### Serial Test Usage

```rust
use serial_test::serial;

#[test]
#[serial]
fn test_requires_exclusive_access() {
    // This test runs alone
}
```

