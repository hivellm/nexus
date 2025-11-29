# Contributing to n8n-nodes-nexus

Thank you for your interest in contributing to the Nexus n8n node! This document provides guidelines for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Submitting Changes](#submitting-changes)
- [Code Style](#code-style)
- [Documentation](#documentation)

## Code of Conduct

By participating in this project, you agree to maintain a respectful and collaborative environment.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally
3. Set up the development environment
4. Create a new branch for your changes
5. Make your changes
6. Test your changes
7. Submit a pull request

## Development Setup

### Prerequisites

- Node.js 18.x or higher
- npm 9.x or higher
- Git
- A running Nexus instance for testing

### Setup Steps

```bash
# Clone the repository
git clone https://github.com/<your-username>/nexus.git
cd nexus/sdks/n8n

# Install dependencies
npm install

# Build the project
npm run build

# Run tests
npm test

# Watch mode for development
npm run dev
```

### Testing with n8n

To test the node in a real n8n environment:

```bash
# Link the package globally
npm link

# In another terminal, install n8n
npm install -g n8n

# Link the package to n8n
cd ~/.n8n
npm link @hivellm/n8n-nodes-nexus

# Start n8n
n8n start
```

## Making Changes

### Branch Naming

Use descriptive branch names:
- `feature/add-new-operation` - For new features
- `fix/connection-timeout` - For bug fixes
- `docs/improve-readme` - For documentation
- `refactor/cleanup-client` - For refactoring

### Commit Messages

Follow conventional commits format:

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

Examples:
```
feat(operations): add vector search operation

Add support for vector similarity search using Nexus KNN index.
Includes parameter configuration for distance metrics and top-k results.

Closes #123
```

```
fix(client): handle connection timeout properly

Improve error handling for connection timeouts with retry logic.
```

## Testing

### Running Tests

```bash
# Run all tests
npm test

# Run tests in watch mode
npm run test:watch

# Run tests with coverage
npm run test:coverage

# Run specific test file
npm test -- NexusClient.test.ts
```

### Writing Tests

All new features should include tests:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { NexusClient } from '../nodes/Nexus/NexusClient';

describe('NexusClient', () => {
  it('should execute query successfully', async () => {
    const client = new NexusClient({
      host: 'localhost',
      port: 15474,
      apiKey: 'test-key',
    });

    const result = await client.executeCypher('RETURN 1 as test');
    expect(result.rows).toHaveLength(1);
  });
});
```

### Test Coverage

Maintain test coverage above 80%:
- Unit tests for all operations
- Integration tests for critical paths
- Error handling tests

## Submitting Changes

### Pull Request Process

1. **Update Documentation**
   - Update README.md if needed
   - Update CHANGELOG.md
   - Add JSDoc comments to new functions

2. **Ensure Tests Pass**
   ```bash
   npm test
   npm run lint
   ```

3. **Create Pull Request**
   - Use a descriptive title
   - Reference related issues
   - Describe your changes
   - Include testing steps

4. **Pull Request Template**
   ```markdown
   ## Description
   Brief description of changes

   ## Type of Change
   - [ ] Bug fix
   - [ ] New feature
   - [ ] Breaking change
   - [ ] Documentation update

   ## Testing
   Describe how you tested your changes

   ## Checklist
   - [ ] Tests added/updated
   - [ ] Documentation updated
   - [ ] CHANGELOG.md updated
   - [ ] All tests passing
   - [ ] Linting passes
   ```

5. **Review Process**
   - Address reviewer feedback
   - Keep discussions constructive
   - Update PR as needed

## Code Style

### TypeScript

Follow the TypeScript style guide:

```typescript
// Use PascalCase for classes
class NexusClient {
  // Use camelCase for methods
  async executeCypher(query: string): Promise<QueryResult> {
    // Use descriptive variable names
    const queryResult = await this.request('/cypher', {
      method: 'POST',
      body: { query },
    });

    return queryResult;
  }
}

// Use interfaces for types
interface QueryOptions {
  parameters?: Record<string, any>;
  timeout?: number;
}

// Use JSDoc comments
/**
 * Execute a Cypher query
 * @param query - The Cypher query to execute
 * @param options - Optional query parameters
 * @returns Query execution result
 */
```

### Linting

```bash
# Run ESLint
npm run lint

# Auto-fix issues
npm run lint:fix

# Format with Prettier
npm run format
```

### Code Organization

```
nodes/
  Nexus/
    Nexus.node.ts       # Main node implementation
    NexusClient.ts      # HTTP client
    operations/         # Operation implementations
      query.ts
      node.ts
      relationship.ts
    types.ts            # Type definitions
    utils.ts            # Utility functions
credentials/
  NexusApi.credentials.ts
  NexusUser.credentials.ts
tests/
  NexusClient.test.ts
  operations.test.ts
  credentials.test.ts
```

## Documentation

### Inline Documentation

Use JSDoc for all public methods:

```typescript
/**
 * Create a new node in the graph
 *
 * @param labels - Array of label strings to assign to the node
 * @param properties - Object containing node properties
 * @returns Created node with ID and properties
 *
 * @example
 * ```typescript
 * const node = await client.createNode(
 *   ['Person'],
 *   { name: 'Alice', age: 30 }
 * );
 * console.log(node.id); // "1"
 * ```
 */
async createNode(
  labels: string[],
  properties: Record<string, any>
): Promise<NodeResult> {
  // Implementation
}
```

### README Updates

When adding new operations:
1. Add to operations list
2. Add usage example
3. Update workflow examples if relevant

### CHANGELOG Updates

Add entries for all changes:

```markdown
## [0.11.1] - 2024-01-15

### Added
- Vector search operation for KNN queries
- Support for geospatial queries

### Fixed
- Connection timeout handling
- Parameter binding for complex types

### Changed
- Improved error messages
- Updated dependencies
```

## Adding New Operations

### Step 1: Define Operation

Add to `Nexus.node.ts`:

```typescript
{
  name: 'vectorSearch',
  value: 'vectorSearch',
  description: 'Perform vector similarity search',
},
```

### Step 2: Add Operation Fields

```typescript
{
  displayName: 'Operation',
  name: 'operation',
  type: 'options',
  noDataExpression: true,
  displayOptions: {
    show: {
      operation: ['vectorSearch'],
    },
  },
  default: 'vectorSearch',
  options: [
    // Operation-specific fields
  ],
},
```

### Step 3: Implement Operation Logic

```typescript
if (operation === 'vectorSearch') {
  const vector = this.getNodeParameter('vector', i) as number[];
  const topK = this.getNodeParameter('topK', i, 10) as number;

  result = await client.vectorSearch(vector, topK);
}
```

### Step 4: Add Client Method

In `NexusClient.ts`:

```typescript
async vectorSearch(
  vector: number[],
  topK: number
): Promise<VectorSearchResult> {
  return this.request('/knn/search', {
    method: 'POST',
    body: { vector, top_k: topK },
  });
}
```

### Step 5: Add Tests

```typescript
describe('vectorSearch', () => {
  it('should return top K similar vectors', async () => {
    const vector = [0.1, 0.2, 0.3];
    const result = await client.vectorSearch(vector, 5);
    expect(result.neighbors).toHaveLength(5);
  });
});
```

### Step 6: Update Documentation

- Add to README operations list
- Add usage example
- Update CHANGELOG

## Release Process

Releases are managed by maintainers:

1. Update version in `package.json`
2. Update CHANGELOG.md
3. Create git tag
4. Push to GitHub
5. Publish to npm

## Questions?

- Open an issue for bugs or feature requests
- Join discussions in GitHub Discussions
- Check existing issues and PRs

Thank you for contributing! 🎉
