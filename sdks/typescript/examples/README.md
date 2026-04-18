# Nexus TypeScript SDK Examples

This directory contains examples demonstrating how to use the Nexus TypeScript SDK.

## Running Examples

1. Install dependencies:
```bash
npm install
```

2. Build the SDK:
```bash
npm run build
```

3. Set up environment variables (optional):
```bash
export NEXUS_URL="http://localhost:7687"
export NEXUS_API_KEY="your-api-key"
```

4. Run an example:
```bash
npx tsx examples/basic-usage.ts
```

or

```bash
npx tsx examples/advanced-queries.ts
```

## Available Examples

- `basic-usage.ts` - Demonstrates basic CRUD operations with nodes and relationships
- `advanced-queries.ts` - Shows complex Cypher queries including pattern matching, aggregations, and path finding

## Example Requirements

- Nexus server running on `http://localhost:7687` (or configured via `NEXUS_URL`)
- Valid API key or username/password for authentication

