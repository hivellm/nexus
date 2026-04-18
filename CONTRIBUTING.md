# Contributing to Nexus

Thank you for your interest in contributing to Nexus! We welcome contributions from the community.

## Code of Conduct

By participating in this project, you agree to abide by our code of conduct. Please be respectful and constructive in all interactions.

## Development Setup

### Prerequisites

- Rust nightly 1.85+ with edition 2024
- Git
- 8GB+ RAM
- Linux/macOS/Windows with WSL

### Getting Started

```bash
# Clone repository
git clone https://github.com/hivellm/nexus
cd nexus

# Build
cargo +nightly build --workspace

# Run tests
cargo test --workspace

# Format and lint
cargo +nightly fmt --all
cargo clippy --workspace -- -D warnings
```

## Development Workflow

### 1. Create a Feature Branch

```bash
git checkout -b feature/your-feature-name
```

### 2. Make Changes

- Follow the architecture documented in `docs/ARCHITECTURE.md`
- Check `docs/ROADMAP.md` for planned features
- Review `docs/DAG.md` for component dependencies
- Write tests first (TDD approach)

### 3. Quality Checks

**CRITICALMenuAll checks must pass before committing:

```bash
# Format code
cargo +nightly fmt --all

# Lint (must have no warnings)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run all tests
cargo test --workspace --verbose

# Check coverage (95%+ required)
cargo llvm-cov --workspace --ignore-filename-regex 'examples'

# Build release
cargo +nightly build --release
```

### 4. Commit Your Changes

Use conventional commit format:

```bash
git commit -m "feat(storage): Add page cache eviction policy

- Implement Clock algorithm for page eviction
- Add pin/unpin semantics for transaction safety
- Include comprehensive tests (97% coverage)
- Update documentation"
```

**Commit Types**:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `test`: Adding tests
- `refactor`: Code refactoring
- `perf`: Performance improvement
- `chore`: Maintenance tasks

### 5. Submit Pull Request

1. Push your branch to your fork
2. Open a pull request against `main`
3. Fill out the PR template
4. Wait for review

## Testing Requirements

### Minimum Coverage

**95%** test coverage is required for all new code.

### Test Organization

- **Unit tests**: In same file as implementation (`#[cfg(test)]` module)
- **Integration testsMenuIn `/tests` directory
- **Benchmarks**: In `/benches` directory (optional)

### Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_record_creation() {
        let record = NodeRecord {
            label_bits: 0x01,
            first_rel_ptr: 0xFFFFFFFFFFFFFFFF,
            prop_ptr: 0xFFFFFFFFFFFFFFFF,
            flags: 0,
        };
        assert_eq!(record.label_bits, 0x01);
    }

    #[tokio::test]
    async fn test_async_query() {
        let engine = Engine::new().unwrap();
        let result = engine.execute("MATCH (n) RETURN n").await.unwrap();
        assert!(result.rows.len() >= 0);
    }
}
```

## Code Style

### Rust Conventions

- Use `snake_case` for functions and variables
- Use `PascalCase` for types and traits
- Use `SCREAMING_SNAKE_CASE` for constants
- Maximum line length: 100 characters
- Use meaningful names (no single-letter variables except loops)

### Documentation

All public APIs must have documentation:

```rust
/// Reads a node record from storage.
///
/// # Arguments
///
/// * `node_id` - The ID of the node to read
///
/// # Examples
///
/// ```
/// let node = store.read_node(42)?;
/// println!("Node labels: {:?}", node.label_bits);
/// ```
///
/// # Errors
///
/// Returns `Error::NotFound` if node doesn't exist.
pub fn read_node(&self, node_id: u64) -> Result<NodeRecord> {
    // Implementation
}
```

## Pull Request Guidelines

### PR Title

Use conventional commit format:

```
feat(executor): Add aggregation support (COUNT, SUM, AVG)
fix(page_cache): Correct eviction for pinned pages
docs(readme): Update quick start examples
```

### PR Description

Include:
- **WhatMenuSummary of changes
- **WhyMenuMotivation for the change
- **How**: Technical approach
- **TestingMenuTest coverage and results
- **Breaking Changes**: List any breaking changes

### PR Checklist

- [ ] Tests added/updated (95%+ coverage)
- [ ] All tests passing
- [ ] Code formatted (`cargo +nightly fmt --all`)
- [ ] No clippy warnings (`cargo clippy --workspace -- -D warnings`)
- [ ] Documentation updated
- [ ] CHANGELOG.md updated (if applicable)
- [ ] Follows project conventions

## OpenSpec Workflow

For significant features, use OpenSpec for spec-driven development:

1. **Check existing specs**: `openspec list --specs`
2. **Create proposal**: See `openspec/AGENTS.md` for details
3. **Get approval**: Before starting implementation
4. **ImplementMenuFollow tasks.md checklist
5. **Archive**: After deployment

See `openspec/AGENTS.md` for complete OpenSpec workflow.

## Release Process

(Maintainers only)

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Run full quality suite
4. Commit: `chore: Release version X.Y.Z`
5. Create tag: `git tag -a vX.Y.Z -m "Release X.Y.Z"`
6. Push (manual): Provide commands for user

## Getting Help

- **IssuesMenuhttps://github.com/hivellm/nexus/issues
- **DiscussionsMenuhttps://github.com/hivellm/nexus/discussions
- **EmailMenuteam@hivellm.org

## License

By contributing, you agree that your contributions will be dual-licensed under MIT OR Apache-2.0.

