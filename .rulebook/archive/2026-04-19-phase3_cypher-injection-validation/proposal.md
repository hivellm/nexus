# Proposal: phase3_cypher-injection-validation

## Why

Several HTTP handlers build Cypher queries via `format!("MATCH (n:{}) ...",
user_input)`. The user-supplied label / property name / rel type flows
straight into the query string without validation:

- `nexus-server/src/api/schema.rs:317` — `format!("CREATE INDEX FOR (n:{})
  ON (n.{})", label, property)`
- `nexus-server/src/api/ingest.rs:307` — node creation with interpolated
  labels
- `nexus-server/src/api/knn.rs:102` — `format!("MATCH (n:{}) RETURN n",
  request.label)`
- `nexus-server/src/api/graphql/resolver.rs` — `rel_type` interpolation in
  relationship resolvers

A malicious client can send `"Person) DETACH DELETE n //` and escape the
pattern. Even if the Cypher dialect doesn't allow every SQL-style trick,
the parser still happily accepts a path that drops everything. Validation
is cheap — labels and property keys are syntactic identifiers.

## What Changes

- Add a single helper `validate_identifier(s: &str) -> Result<&str>` (in
  `nexus-server/src/api/` or `nexus-protocol/`) that enforces
  `^[A-Za-z_][A-Za-z0-9_]*$`.
- Call the helper at every user-input-to-query boundary before `format!`.
- For the GraphQL resolver, apply the same check on `rel_type`.
- Add a regression test that sends `"Person) MATCH (m) DETACH DELETE m //"`
  and asserts the endpoint returns a 400, not a 200.

## Impact

- Affected specs: `docs/AUTHENTICATION.md`, `docs/SECURITY_AUDIT.md`
- Affected code: schema.rs:317, ingest.rs:307, knn.rs:102,
  graphql/resolver.rs:153-176 (and similar call sites in
  `graph_correlation.rs`)
- Breaking change: YES for clients sending label/property strings that
  contain characters outside `[A-Za-z0-9_]` — they were already exploiting
  the bug
- User benefit: closes an injection hole that is today exploitable with
  a single HTTP POST
