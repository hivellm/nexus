# Proposal: phase8_jvm-sdk-java-kotlin

## Why

Nexus ships 6 first-party SDKs (Rust, TypeScript, Python, Go, C#, PHP) at v1.15.0 — but **no JVM SDK**. The Bolt-driver world (Neo4j's largest user base, Spring Data, GraphRAG-on-JVM stacks, every enterprise Java app) lives on the JVM. This is the single biggest enterprise-adoption gate for Nexus today. Without a Java SDK, every existing Neo4j Java app would need a rewrite to migrate. With one, migration is "swap the dependency".

## What Changes

- Build a JVM SDK in Kotlin (interoperable with Java 11+) under `sdks/jvm/`.
  - Idiomatic Kotlin coroutines API (`suspend fun query(...)`).
  - Java-friendly `CompletableFuture<QueryResult>` + blocking `query(...)` overloads.
- Implement RPC transport (binary MessagePack over `nexus://` URL) and HTTP transport.
- Match the existing Rust SDK's surface: `connect`, `query`, `db.list/create/drop/switch`, `user.*`, `key.*`, `schema.*`, `data.*`.
- Comprehensive test suite (≥ 30 tests, JUnit 5 / Kotlin Test) covering CRUD, parameterized queries, aggregations, KNN, FTS, transactions.
- Publish to Maven Central as `org.hivellm:nexus-sdk:1.16.0` (next minor — bumps SDK train).
- Optional add-on (defer to follow-up): `org.hivellm:nexus-spring-data` for Spring Data integration.

## Impact

- Affected specs: `docs/sdks/JVM.md` (new), README highlights.
- Affected code: new `sdks/jvm/` directory (Gradle Kotlin multi-module), CI publish workflow.
- Breaking change: NO (additive).
- User benefit: closes the largest enterprise-adoption gate; existing Neo4j Java apps can migrate by swapping the dependency.
