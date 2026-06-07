## 1. Investigation
- [x] 1.1 Define the contract for a null property value in MERGE/index: does `MERGE (n:L {id: null})` create a phantom null-keyed node? Should null keys be excluded from the property index or rejected?
- [x] 1.2 Confirm a clean wipe+rebuild path works (DROP DATABASE / bulk clear) and that indexes + adjacency rebuild correctly
- [x] 1.3 Decide whether any Nexus code change is required or §4 is purely a downstream re-bootstrap (docs-only)

## 2. Implementation
- [x] 2.1 Enforce the null-key contract in MERGE/index (exclude or reject null property keys consistently across write + read seek)
- [x] 2.2 Ensure the clean-rebuild path is correct (drop -> recreate indexes -> re-ingest), or document it if no code change is needed

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the rebuild procedure
- [x] 3.2 Write tests covering the null-key contract (and rebuild correctness if code changes)
- [x] 3.3 Run tests and confirm they pass
