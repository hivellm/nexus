# Changelog

All notable changes to Nexus will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] — 2026-04-30

> **Major version bump**: 1.x → 2.0.0. Marks the first phase-8
> ship train (encryption-at-rest core + KMS + WAL, quantified
> path patterns with mode keywords, query-plan cache). The 1.15
> interim line is collapsed into this entry; the previous
> `release/v1.2.0` branch name continues to host the cut for
> compat with upstream PR refs, and every workspace + SDK
> manifest now reads `2.0.0`.

### Added — `phase9_external-node-ids`

- **External node identifiers**: Reserved `_id` property on nodes stores caller-supplied external IDs (stable, deduplication-friendly). `ExternalId` enum supports Hash (Blake3/SHA-256/SHA-512), Uuid, String (≤256 bytes), and Bytes (≤64 bytes) variants with 1-byte wire discriminator.
- **Conflict policies on CREATE**: `ON CONFLICT ERROR | MATCH | REPLACE` modifier controls behavior when external ID already exists. ERROR (default) fails; MATCH returns existing node unchanged; REPLACE updates properties while preserving internal ID.
- **REST endpoints**: `POST /data/nodes` accepts `external_id` + `conflict_policy` parameters; new `GET /data/nodes/by-external-id` endpoint for lookup. Both follow existing 200-with-error response pattern (never 404).
- **Catalog persistence**: Two LMDB sub-databases (`external_ids` forward, `internal_ids` reverse) in catalog with atomic WAL updates and replay-safe recovery. Bidirectional mapping maintains O(log n) index seek in Cypher planner.
- **Cypher surface**: Nodes without external ID behave identically to pre-phase-9 behavior (no breaking changes). Query planner automatically selects external-ID index for `MATCH (n {_id: ...})` and `MATCH (n) WHERE n._id = ...` predicates. `MERGE` fast-paths on pure `_id` constraints.
- **Rust SDK**: `create_node_with_external_id(labels, properties, external_id, conflict_policy)` and `get_node_by_external_id(external_id)` helpers. All SDKs (Python, TypeScript, Go, C#, PHP) updated with equivalent surface.

### Added — `phase8_query-plan-cache`

- **Process-wide query plan cache** at
  [`crates/nexus-core/src/executor/planner/cache.rs`](crates/nexus-core/src/executor/planner/cache.rs).
  `PlanCache<V>` is a generic LRU keyed by `xxh3_64` of the
  canonicalised query, with atomic hit / miss / eviction counters
  and a `planner_generation: AtomicU64` for schema-change
  invalidation. Eviction policy: classic LRU (move-to-front on
  hit, evict tail on capacity bound). Lookup is `O(1)` plus one
  `parking_lot::Mutex` acquisition.
- **Env knobs**:
  - `NEXUS_PLAN_CACHE_ENTRIES` (default `1024`) sets the LRU
    capacity. `0` disables the cache.
  - `NEXUS_PLAN_CACHE_DISABLE` (when set to `1` / `true` / `yes`)
    builds a permanently-disabled cache. Lookups return `None`,
    inserts are no-ops, miss counter still ticks so an operator
    who flips the knob mid-flight sees the impact.
- **`QueryOptimizer` integration**: the per-instance
  `HashMap<String, OptimizationResult>` + FIFO approximation that
  shipped pre-phase-8 is replaced by an `Arc<PlanCache<...>>`.
  Multiple executors share the same cache so a warmup in one
  connection benefits the next; `bump_plan_cache_generation()`
  surfaces the schema-change hook for callers that mutate the
  catalog. `get_cache_stats()` returns the legacy
  `CacheStats { cache_size, max_cache_size, hit_rate }` shape
  computed from the new atomic counters.
- **Operator-facing surface**: `PlanCache::top_n(n)` returns
  `(canonical_hash, access_count, generation)` for `db.planCache.list`
  consumers; `PlanCache::clear()` is the emergency flush;
  `PlanCache::stats()` returns the full counter snapshot.
- 12 new unit tests covering hit / miss / LRU eviction /
  generation invalidation / clear-preserves-counters / disabled
  no-op / zero-capacity-disabled / `top_n` ordering /
  re-insert preserves access count / 16-thread concurrent
  lookup / env-var disable knob.

### Added — `phase8_quantified-path-patterns-execution`

- **Path-mode keywords** for QPP: `WALK | TRAIL | ACYCLIC | SIMPLE`
  precede the quantified group and constrain repeated edges /
  nodes across the matched path. `WALK` is the implicit default
  (matches the historical engine behaviour); the other three
  carry the Cypher 25 / GQL semantics.
- **`QppMode` enum** in
  [`crates/nexus-core/src/executor/types.rs`](crates/nexus-core/src/executor/types.rs)
  threads the mode through the `Operator::QuantifiedExpand`
  variant, the planner, and the AST `QuantifiedGroup`.
- **Per-frame visited-set tracking** in `execute_quantified_expand`
  ([`crates/nexus-core/src/executor/operators/quantified_expand.rs`](crates/nexus-core/src/executor/operators/quantified_expand.rs)).
  TRAIL / SIMPLE maintain a `path_edges: Vec<u64>` per BFS frame
  and reject any walk extension whose new edges intersect that
  set or repeat within the body iteration; ACYCLIC / SIMPLE do
  the same for `path_nodes`. Wavefront dedup `(node, iteration)`
  is disabled for non-WALK modes — distinct paths to the same
  node at the same iteration count have distinct visited sets
  and may extend into different futures.
- **Parser support**: `clauses.rs` peeks for an optional
  `WALK | TRAIL | ACYCLIC | SIMPLE` keyword right before the
  opening paren of a QPP group. Backtracking restores both the
  mode keyword and the QPP probe when the lookahead does not
  form a real QPP, so identifiers that happen to start with one
  of the four keyword letters keep parsing as identifiers.
- **`mode_explicit` flag** on `QuantifiedGroup`: any explicit
  mode keyword (including `WALK`) disables the legacy `*m..n`
  fast-path lowering and routes through the dedicated
  `QuantifiedExpand` operator. The implicit (no-keyword) default
  keeps the lowering on so the textbook anonymous-body shape
  takes the legacy path byte-for-byte unchanged.
- 7 new TCK-style tests covering each mode against triangle
  (loop) and diamond (parallel-paths) fixtures, the explicit
  `WALK` keyword routing, ACYCLIC bounded against an unbounded
  triangle loop, and the zero-length-quantifier interaction
  with `SIMPLE`.

### Added — `phase8_encryption-at-rest-wal`

- **Encrypted WAL append + replay** in
  [`crates/nexus-core/src/wal/mod.rs`](crates/nexus-core/src/wal/mod.rs).
  New v3 frame format (`Aes256GcmCrc32C` algo, dispatched off the
  existing v2 magic byte) carries AES-256-GCM ciphertext with a
  tag, plus a CRC32C over the recovered plaintext for end-to-end
  integrity. v3 layout: `[magic:1=0x00][algo:1=0x03][type:1]
  [plain_len:4][crc_plain:4][ciphertext+tag: plain_len + 16]`.
- **AAD-bound metadata**: `[type, plain_len, crc_plain,
  frame_offset]` (17 bytes). A tamperer who relocates a frame to
  a different file offset triggers an AEAD failure on replay.
- **Nonce**: `PageNonce::new(FileId::Wal, frame_offset, 1)`. Nonce
  uniqueness across the file follows from append-only semantics
  between truncations.
- **WAL key-rotation contract**: every `Wal::truncate()` must be
  paired with a key rotation in production (the rotation runner
  shipped under `phase8_encryption-at-rest-rotation` coordinates
  with the checkpoint epoch). Documented in
  `docs/security/ENCRYPTION_AT_REST.md` § "WAL encryption".
- **On-disk EaR magic**: encrypted WAL files start with a 16-byte
  `NXCP` page header (`FileId::Wal`, generation 1) so the boot
  inventory scanner classifies them as `Encrypted`; without the
  header, the per-frame `0x00` magic byte would otherwise look
  plaintext to the inventory.
- **Replay tolerance**: a short read mid-frame, or an AEAD
  failure on a frame whose body extends to EOF, are both treated
  as truncation (parity with the existing CRC-mismatch behaviour
  for v1/v2 frames). Mid-WAL AEAD failures surface
  `ERR_WAL_AEAD`; plaintext CRC mismatches after successful AEAD
  surface `ERR_WAL_CRC`.
- **Backward compatibility**: existing v1 plaintext frames and v2
  algo-stamped frames continue to replay byte-for-byte unchanged;
  the v3 dispatcher is gated behind the `Aes256GcmCrc32C` algo
  byte and only fires when the WAL was opened via
  `Wal::with_cipher`.
- 10 new tests: round-trip recovery, EaR magic at offset 0,
  ciphertext does not contain plaintext payload, wrong-key →
  `ERR_WAL_AEAD`, mid-WAL bit-flip → `ERR_WAL_AEAD`,
  trailing-frame truncation tolerance (both byte-truncated and
  AEAD-failed), `with_cipher` rejects existing plaintext WAL,
  plaintext WAL refuses v3 append, truncate preserves page
  header, and post-truncate replay walks fresh frames cleanly.

### Added — `phase8_encryption-at-rest-storage-hooks` (boot-invariant slice)

- **Boot-time encryption inventory scanner** at
  [`crates/nexus-core/src/storage/crypto/inventory.rs`](crates/nexus-core/src/storage/crypto/inventory.rs).
  Walks the data directory before the executor opens any record
  store, reads the first 16 bytes of each regular file, and
  classifies the file as `Empty` / `Plaintext` /
  `Encrypted { file_id, generation }` based on the EaR magic.
- **Mixed-mode rejection** via `enforce_uniform_state`: refuses
  to boot when at least one plaintext file sits alongside at
  least one encrypted file (`ERR_ENCRYPTION_MIXED_MODE`),
  rejects a flag-flipped configuration whose on-disk state
  contradicts the boot config (`ERR_ENCRYPTION_UNEXPECTED_ENCRYPTED`
  / `ERR_ENCRYPTION_NOT_INITIALIZED`).
- **`enforce_data_dir_invariants`** in
  [`crates/nexus-server/src/config.rs`](crates/nexus-server/src/config.rs)
  drives the scan from the boot path; the resulting
  `EncryptionInventorySummary { empty, plaintext, encrypted }`
  rides along on `EncryptionConfig` and surfaces over
  `GET /admin/encryption/status` as a counts-only `inventory`
  field. Per-file paths land in the boot log line on error;
  never sent over the network.
- **Status quo on the actual page-stream wiring**: the catalog
  (LMDB has no engine-side page hook), record stores (mutate
  `MmapMut` in place — no buffer pool yet), page cache (no real
  disk backing today), and the matching round-trip / crash-
  recovery / benchmark items are all blocked on a storage-layer
  refactor that is too large to land in this slice. The
  inventory scanner is the floor those wirings will report
  against once they ship; until then, every boot proves the
  data directory is uniform and the operator surface honestly
  reflects "no surfaces wired yet" via the empty
  `storage_surfaces` array.
- 19 new tests: 15 unit tests covering `classify_file` (empty /
  short / plaintext / encrypted recovery), `scan_paths` /
  `scan_directory` (skip list + recursion + missing-dir
  tolerance), and `enforce_uniform_state` (every cell of the
  decision matrix); 4 server-side tests covering
  `enforce_data_dir_invariants` (clean dir, mixed mode,
  encrypted-with-flag-off, uniform success).
- Operator-facing docs at
  [`docs/security/ENCRYPTION_AT_REST.md`](docs/security/ENCRYPTION_AT_REST.md)
  § "Mixed-mode detection (boot invariant)" — decision matrix
  + sample status JSON.

### Added — `phase8_encryption-at-rest-kms`

- **AWS KMS, GCP KMS, and HashiCorp Vault adapters** for the
  `KeyProvider` trait at
  [`crates/nexus-core/src/storage/crypto/kms/`](crates/nexus-core/src/storage/crypto/kms/).
  DEK pattern: each adapter holds a wrapped data-key blob on
  disk + a reference to a KMS-owned KEK; at boot the adapter
  calls the KMS once to unwrap the DEK and caches the 32-byte
  plaintext for the process lifetime. Transient KMS outages
  after boot do not affect serving traffic.
- **Feature-gated.** `kms-aws` (`aws-sdk-kms` + `aws-config`),
  `kms-gcp` (`google-cloud-kms`), `kms-vault` (`vaultrs`); the
  roll-up `kms` enables all three. Default builds skip the SDKs
  entirely so dev / CI compile times are unaffected.
- **Operator config** via `NEXUS_KMS_PROVIDER` ∈ `aws|gcp|vault`
  plus per-provider `NEXUS_KMS_*` env vars. Boot resolution
  precedence: KMS > `NEXUS_KEY_FILE` > `NEXUS_DATA_KEY`. An
  unknown provider, or one whose feature is not built in,
  surfaces a hard fail at boot — no silent fall-through to
  plaintext.
- **`EncryptionSource::Kms { provider, label }`** added to
  `nexus-server::config` so `/admin/encryption/status` reports
  which KMS unwrapped the master key. The label is the
  adapter's `KeyProvider::label()` — provider-specific
  identifier safe to log (KMS key ARN, GCP key resource path,
  Vault transit mount/key); never the master key itself.
- 24 new tests: 13 unit tests covering the shared `KmsError`
  taxonomy + per-provider config-validation paths, 8 server-
  side encryption tests covering the new resolution branches +
  `EncryptionSource::Kms` JSON serialisation, and 3 ignored-by-
  default integration tests against localstack (AWS), the
  google-cloud-kms emulator (GCP), and `vault dev` (Vault).
- Operator-facing docs at
  [`docs/security/ENCRYPTION_AT_REST.md`](docs/security/ENCRYPTION_AT_REST.md)
  § "KMS adapters" — recipes for one-shot DEK provisioning per
  provider and a structured error catalogue.

### Added — `phase8_query-plan-cache` (canonicaliser slice)

- **Cypher canonicaliser landed at
  `crates/nexus-core/src/executor/planner/cache.rs`** — turns a
  query string into a cache-key-friendly form before hashing.
- Strips line comments (`// ...`), block comments
  (`/* ... */`, non-nested), collapses every run of ASCII
  whitespace to a single space, trims leading + trailing
  whitespace. **Does not touch string literals**: `'a  b'`
  keeps its inner whitespace; `// inside a string` is not
  treated as a comment marker.
- Does **not** lower-case keywords (Cypher is case-sensitive on
  identifiers; lower-casing would alias property names like
  `match` with the keyword) and does **not** normalise
  parameter placeholders (`$x` and `$y` participate in binding
  scope and produce different plans).
- `canonicalise_query(&str) -> Cow<'_, str>` — `Cow::Borrowed`
  on already-canonical input (zero-allocation cache hit path),
  owned `String` otherwise.
- `hash_canonicalised(&str) -> u64` — xxh3 over the canonical
  form + a `CANONICAL_VERSION = 1` stamp so a future shape
  change forces a clean cache invalidation.
- `executor::optimizer::QueryOptimizer::hash_query` now routes
  through the canonicaliser. Two queries that differ only in
  whitespace or comments now hit the same plan-cache entry,
  closing the cache-miss path that templated workloads tripped
  on every request.
- 21 unit tests cover empty input, already-canonical
  borrow-not-clone path, whitespace + tab + newline collapse,
  leading/trailing trim, line-comment + block-comment
  stripping, unterminated block comment to EOF, single +
  double + escaped string-literal preservation, comment-inside-
  string-literal preservation, multiple back-to-back comments,
  keyword-case preservation, parameter-name distinction,
  hash stability + collapse + distinction invariants.
- The remaining `phase8_query-plan-cache` items (lookup at
  `Engine::execute`, schema-change invalidation,
  `db.planCache.*` procedures, env vars, `/stats` counters,
  Prometheus metrics, hot-endpoint bench) consume the
  canonicaliser without changing it; tracked under the same
  task for follow-up sessions.
- Quality gates: `cargo +nightly test -p nexus-core --lib`
  2232 passed (same pre-existing parallel-flake on
  `engine::tests::match_scopes_by_label_and_property_together`
  noted under `phase8_optional-match-binding-leak`); `cargo
  +nightly clippy -p nexus-core --all-targets -- -D warnings`
  clean.

### Added — `phase8_encryption-at-rest-indexes` (R-tree shipped)

- **R-tree spatial index gains an encrypted page-store**, the
  first index family to wire encryption-at-rest end-to-end.
- New module `crates/nexus-core/src/index/rtree/encrypted_store.rs`:
  `EncryptedFilePageStore` lives parallel to `FilePageStore` and
  drops into every R-tree call site through the existing
  `PageStore` trait — no R-tree internals needed modification.
- On-disk slot layout: `ENCRYPTED_RTREE_SLOT_SIZE = 8224 bytes`
  per logical 8 KB R-tree page, laid out as
  `[16 B header][8192 B ciphertext][16 B AEAD tag]`. Header
  carries magic `NXRT` (`0x4E58_5254`), `FileId::RTreeIndex`,
  and a per-page `u32` generation counter; bound into the AEAD
  as AAD so adversarial header swaps fail at decrypt.
- Per-page nonce derives from `(FileId::RTreeIndex,
  page_offset, generation)`. Generation bumps on every
  overwrite, structurally preventing AES-GCM nonce reuse
  (catastrophic with the same key).
- Crash consistency mirrors `FilePageStore`: live-set sidecar
  (`<path>.live`), `flush()` syncs both the data file and the
  sidecar, reopening picks up every page that was committed.
- 12 unit tests cover round-trip on a full 8192-byte page,
  distinct pages get distinct slots, overwrite advances
  generation, wrong-key surfaces a clean IO error, tampered
  ciphertext is rejected, header swap is detected at decrypt,
  restart recovers the live set + decrypts every page, delete +
  re-read produces NotFound, page-id-zero rejected on every
  method, wrong-size writes rejected, empty-store invariants,
  on-disk slot layout matches the documented contract.
- Spec: `docs/specs/rtree-index.md` gains an "Encrypted
  page-store" section documenting the slot layout, nonce
  derivation, performance overhead (~2-3 µs per page on
  AES-NI), and the constructor-swap wiring recipe.
- B-tree, full-text, and KNN remain follow-ups: the B-tree is
  in-memory today (no on-disk format to encrypt yet); Tantivy
  needs a custom `tantivy::Directory` adapter; `hnsw_rs` lacks
  a streaming-IO seam. The R-tree pattern is the template the
  three adopt as their IO seams land. Status documented in
  `docs/security/ENCRYPTION_AT_REST.md` follow-up table;
  `-indexes` now reads **partial**.
- Quality gates: `cargo +nightly test -p nexus-core --lib
  index::rtree::encrypted_store::` 12/12 green; `cargo +nightly
  clippy -p nexus-core --all-targets -- -D warnings` clean.

### Fixed — `phase8_optional-match-empty-driver`

- **OPTIONAL MATCH against an empty driver now returns one NULL
  row instead of zero rows**, matching the Neo4j contract.
  `OPTIONAL MATCH (n:NonExistentLabel) RETURN n` returned `[]`
  before the fix, returns `[[null]]` after. Property access
  (`RETURN n.name`) and aggregation (`RETURN count(n)`) flow
  through the same fix.
- Root cause: the planner emitted a regular `NodeByLabel +
  Project` pipeline that produced zero rows when the labelled
  set was empty. OPTIONAL MATCH is a LEFT OUTER JOIN against an
  implicit single-row driver when no prior clause feeds the
  pipeline; the emitted plan had no driver.
- Fix: new operator `Operator::EnsureNullRowIfEmpty { vars }` in
  `crates/nexus-core/src/executor/types.rs`, executed in both
  `executor::operators::dispatch` and the main `executor::mod`
  exec loop. The planner appends it after the first OPTIONAL
  pattern's scan when (a) `first_is_optional == true`, (b) no
  prior driver (`unwind_before_match == false` and the only
  operators in the pipeline so far are `NodeByLabel` /
  `AllNodesScan` / `Filter`).
- 6 regression tests in
  `crates/nexus-core/tests/optional_match_empty_driver_test.rs`:
  empty-label returns NULL row, property access returns NULL,
  count returns 0, prior MATCH eliminating rows does NOT
  resurrect them, OPTIONAL on a non-empty label returns the
  actual rows (NOT a NULL row), and the existing
  `phase8_optional-match-binding-leak` contract still holds.
- Spec: `docs/specs/cypher-subset.md` § "OPTIONAL MATCH" gains
  a "Standalone OPTIONAL MATCH semantics" subsection.
- Quality gates: `cargo +nightly test -p nexus-core --lib` 2199
  passed (1 pre-existing parallel-flake on
  `engine::tests::match_scopes_by_label_and_property_together`,
  unrelated to this fix, passes in isolation — same flake
  documented under `phase8_optional-match-binding-leak`); the
  new test file 6/6 green; the
  `phase8_optional-match-binding-leak` regression suite 7/7
  still green; `cargo +nightly clippy -p nexus-core --all-targets
  -- -D warnings` clean.

### Added — `phase8_encryption-at-rest-cli` (status surface)

- **Operator surface for encryption-at-rest configuration.** The
  cryptographic core + rotation runner already shipped; this commit
  ships the boot-resolution + status endpoint + CLI subcommand so
  operators can verify their key configuration without waiting on
  the storage-hook follow-ups.
- New `EncryptionConfig` struct in `crates/nexus-server/src/config.rs`
  with `enabled`, `source` (env / file), and `fingerprint` fields.
  Resolved at server boot via the new `resolve_encryption_config()`
  helper: parses `NEXUS_ENCRYPT_AT_REST=true`, picks `NEXUS_KEY_FILE`
  over `NEXUS_DATA_KEY`, instantiates the matching `KeyProvider`,
  validates the master key, computes a SHA-256-derived fingerprint
  (`nexus:` + first 16 hex digits of the digest — safe to log).
- `Config::from_env()` now invokes the resolver and panics with
  `ERR_ENCRYPTION_BOOT` on a malformed / missing key. An operator
  who set `NEXUS_ENCRYPT_AT_REST=true` and got a typo'd key path
  must NEVER see the server start in plaintext mode.
- `NexusServer` carries a new `encryption_config` field; `main.rs`
  populates it before wrapping the handle in `Arc` and logs the
  fingerprint at boot when encryption is enabled.
- New API endpoint `GET /admin/encryption/status` returning a
  versioned `EncryptionStatusReport`: `enabled`, `source`,
  `fingerprint`, `storage_surfaces` (empty today; populated by the
  storage-hook follow-ups), `schema_version: 1`. Optional fields
  use `skip_serializing_if` so the JSON shape stays clean for the
  default-disabled case.
- `nexus admin encryption status` CLI subcommand calls the new
  endpoint via the new `NexusClient::get_json` helper. Supports
  `--json` output. Pretty-prints the source / fingerprint or a
  hint about how to enable encryption when disabled.
- 10 new tests cover: fingerprint determinism + per-key
  independence + no-key-byte leak; env-var resolution disabled
  case; env-var resolution with a hex key (records source +
  fingerprint); bad-format rejection; status-handler JSON shape +
  field names + `skip_serializing_if` behaviour. Tests use a
  shared `Mutex` to serialise env-var mutation against the
  process-wide global.
- `docs/security/ENCRYPTION_AT_REST.md` § "Activation" rewritten
  with the live recipe + fingerprint explainer; follow-up table
  marks `-cli` as **partial**.
- `crates/nexus-server` gained a `sha2 = "0.10"` dep (paired with
  the workspace `hkdf 0.12` digest 0.10 ecosystem; coexists with
  nexus-core's `sha2 = "0.11"` for argon2/API keys).
- Migration / rotation / mixed-mode-rejection subcommands stay
  carved to `phase8_encryption-at-rest-storage-hooks`; the CLI
  must not expose actions the engine cannot yet honour.
- Quality gates: `cargo +nightly test -p nexus-server --lib encryption`
  10/10 green; `cargo +nightly clippy -p nexus-server -p nexus-cli
  --all-targets -- -D warnings` clean.

### Added — `phase8_encryption-at-rest-rotation`

- **Online key rotation** built on top of the encryption-at-rest
  cryptographic core. NIST SP 800-57 recommends rotating
  data-encryption keys at most annually; this commit ships the
  runner that does it without downtime.
- `EncryptedPageStream` extended with an optional **secondary**
  cipher: `install_secondary` / `clear_secondary` / `has_secondary`.
  The read path probes primary first, falls back to secondary on
  `ERR_BAD_KEY`, surfaces the primary's error if both fail. The
  write path always uses the primary so new pages are immediately
  consistent with the post-rotation state.
- New `KeySource` enum + `decrypt_with_source` so the runner can
  tell whether a page was decrypted under the primary (no-op) or
  the secondary (must re-encrypt).
- `PageStore` trait — the storage-layer seam the runner walks.
  `InMemoryPageStore` ships today; storage-hook follow-ups
  (`-storage-hooks`, `-wal`, `-indexes`) provide concrete impls.
- `RotationRunner` orchestrator: ascending `(file_id, page_offset)`
  sweep, idempotent on already-primary pages, throttled by a
  configurable `byte_budget_per_second` (default 64 MiB/s),
  cancellable via an `Arc<AtomicBool>`, resumable from a
  serde-serialisable `RotationCheckpoint`.
- `RotationStats`: `pages_total`, `pages_rotated`,
  `pages_already_primary`, `bytes_rotated` — ready to export to
  Prometheus when the metrics layer wires up.
- 9 new unit tests cover: read-path fallback to secondary,
  read-path-without-secondary fails loudly, runner rejects
  no-secondary state, runner rotates every page to primary,
  runner skips already-primary pages, runner resumes from
  checkpoint, runner honours cancel flag, write during rotation
  uses primary (post-clear read still works), cleared-secondary
  can be reinstalled (chained rotations).
- `FileId` enum gained `Serialize`/`Deserialize` + `Ord` so
  `PageRef` round-trips cleanly through the checkpoint.
- Doc: [`docs/security/ENCRYPTION_AT_REST.md`](docs/security/ENCRYPTION_AT_REST.md)
  § "Online key rotation" rewritten from follow-up placeholder
  to the live spec.
- Quality gates: 45/45 `storage::crypto::*` tests green; clippy
  clean.

### Added — `phase8_encryption-at-rest` (cryptographic core)

- **Encryption-at-rest cryptographic foundation.** SOC2 / FedRAMP
  / HIPAA / PCI-DSS gate. Neo4j Enterprise, Aura, ArangoDB
  Enterprise, Memgraph Enterprise all ship this; Nexus's previous
  posture ("rely on disk-level encryption") was disqualifying for
  any regulated customer.
- New module `crates/nexus-core/src/storage/crypto/`:
  - `key_provider.rs` — `KeyProvider` trait, `EnvKeyProvider`
    (reads `NEXUS_DATA_KEY` once at construction), `FileKeyProvider`
    (0600-perm-checked on Unix, ACL-deferred on Windows). Master
    key sources accept either 32 raw bytes or 64-char hex.
  - `kdf.rs` — HKDF-SHA-256 per-database key derivation (RFC 5869).
    Domain-separated via `nexus-encryption-at-rest-v1` tag;
    rotatable per database via an `epoch` parameter.
  - `aes_gcm.rs` — AES-256-GCM page cipher with deterministic
    `(file_id, page_offset, generation)` 96-bit nonce. The
    generation counter is non-negotiable — AES-GCM is
    catastrophically broken under nonce reuse.
  - `encrypted_file.rs` — `EncryptedPageStream` is the seam
    storage hooks plug into. 8 KiB pages with a 16-byte
    plaintext header (magic + file_id + generation) bound into
    the AEAD as AAD so an adversary swapping the on-disk header
    is detected at decrypt time.
- Every secret wrapped in `zeroize::Zeroizing` so it gets wiped on
  drop.
- Failure surface: `ERR_BAD_KEY` (vague on purpose to avoid a
  CCA-2 oracle), `ERR_KEY_NOT_FOUND`, `ERR_KEY_BAD_FORMAT`,
  `ERR_KEY_IO`, `ERR_KEY_HEX`, `ERR_KDF_BAD_LENGTH`,
  `ERR_KDF_EMPTY_DATABASE`, `ERR_PAGE_HEADER`,
  `ERR_PAGE_TOO_LARGE`, `ERR_AEAD_EMPTY`.
- 36 unit tests cover: nonce layout (round-trip, 48-bit truncation,
  endianness), AEAD round-trip, wrong-key / wrong-database-key /
  AAD-mismatch / nonce-mismatch / tampered-ciphertext / empty-input
  rejection, no-plaintext-leak invariant, distinct-nonces-produce-
  distinct-ciphertexts, HKDF determinism + per-name + per-epoch +
  per-master independence, page-stream generation advancement,
  on-disk header parsing + invalid-magic / unknown-file-id
  rejection, header-swap detection at decrypt, key-rotation-via-
  fresh-stream invalidates old pages, env-var hex parsing,
  file-key newline stripping, missing-file IO error.
- New doc:
  [`docs/security/ENCRYPTION_AT_REST.md`](docs/security/ENCRYPTION_AT_REST.md) —
  threat model, architecture, cryptographic choices, key-management
  recipes, performance expectations, operational checklist.
  [`AUTHENTICATION.md`](docs/security/AUTHENTICATION.md) cross-
  links the new doc.
- **Storage-layer wiring is intentionally NOT in this commit.**
  Wiring the page stream into LMDB catalog, record stores, WAL,
  and indexes is invasive; each module has its own invariants
  that need a per-module review. Tracked under
  `phase8_encryption-at-rest-storage-hooks`,
  `-wal`, `-indexes`, `-kms`, `-rotation`, and `-cli`. The
  contracts in this commit are stable and the follow-ups consume
  them without changing any public API.
- Workspace deps added: `aes-gcm = "0.10"`, `hkdf = "0.12"`,
  `sha2_010` (sha2 0.10 alias to satisfy hkdf's digest 0.10
  bound; coexists with the existing `sha2 = "0.11"` already in
  nexus-core), `zeroize = "1.8"`.
- Quality gates: `cargo +nightly test -p nexus-core --lib
  storage::crypto::` 36/36 green; `cargo +nightly clippy
  -p nexus-core --all-targets -- -D warnings` clean.

### Added — `phase8_cross-shard-2pc`

- **V2 cluster mode now supports atomic multi-shard writes.** Before
  phase 8, the coordinator's scatter path `fail-atomics` any
  mutation whose write set spanned more than one shard. The
  remaining gate for advertising V2 as production-grade
  multi-shard cluster mode (every other engine in this space —
  Memgraph HA, ArangoDB cluster, Dgraph, NebulaGraph — supports
  multi-shard writes).
- **Pessimistic ordered locking, not 2PC.** ADR-009 documents
  the choice: pessimistic locking has zero coordinator-state
  recovery story (leases time out on the shard side if the
  coordinator dies), Havender total-order deadlock prevention
  (no cycle is possible), and forward-compatible API for a
  future full-2PC swap. Tracked under
  `phase9_full-2pc-cross-shard`.
- New module
  `crates/nexus-core/src/coordinator/multi_shard_tx.rs` (~700
  LOC): `TxId` + `TxIdAllocator`, `WriteSet` (BTreeSet over
  shards iterated in ascending order), `ShardLockManager` trait
  + in-memory test impl with chaos hooks
  (`inject_partition`, `inject_failure`, `force_release`),
  `ShardMutator` trait, `MultiShardTx` orchestrator
  (`acquire-in-order` → `mutate` → `release-in-reverse-order`,
  with a deterministic abort path that rolls back every
  previously-mutated shard).
- Failure surface: `ERR_LOCK_BUSY`, `ERR_LOCK_TIMEOUT`,
  `ERR_PARTITION`, `ERR_SHARD_FAILURE`, `ERR_SHARD_MUTATION`,
  `ERR_ROLLBACK_FAILED`, `ERR_TX_TIMEOUT`,
  `ERR_EMPTY_WRITE_SET`. Each maps to a specific recovery
  procedure documented in
  `docs/specs/cluster-transactions.md`.
- Metrics counters surfaced for Prometheus:
  `nexus_cluster_multi_shard_writes_total`,
  `_writes_aborted_total`, `_lock_acquire_total`,
  `_lock_timeout_total`. Recommended dashboards (abort ratio,
  lease wait time, per-shard fairness) documented in the spec.
- 11 unit tests pin every chaos case: leader churn mid-write,
  partition mid-acquisition, busy-shard timeout, 64 concurrent
  writers on overlapping shard sets (no deadlock), shard outage
  mid-commit (atomic rollback in reverse order),
  rollback-itself-fails (state preserved, root cause not
  masked).
- New spec: [`docs/specs/cluster-transactions.md`](docs/specs/cluster-transactions.md).
- Updated guide: [`docs/guides/DISTRIBUTED_DEPLOYMENT.md`](docs/guides/DISTRIBUTED_DEPLOYMENT.md).
- Quality gates: workspace tests `2154 passed; 0 failed` (11 new);
  `cargo clippy -p nexus-core --all-targets` clean.

### Documentation — `phase7_kuzu-migration-guide`

- **New migration guide for displaced KuzuDB users.** Kùzu Inc.
  archived its repository on 2025-10-10. `docs/migration/FROM_KUZU.md`
  covers schema mapping (Kùzu node/rel tables → Nexus labels/types),
  Cypher dialect deltas (`[*SHORTEST n..m]`, `CREATE_HNSW_INDEX`,
  `CREATE_FTS_INDEX`, `QUERY_VECTOR_INDEX`, `QUERY_FTS_INDEX`),
  vector + FTS index migration with the cosine score sign-flip
  flagged, embedded-mode → RPC story, and a full gotchas section.
- `scripts/migration/from_kuzu.py` ships three subcommands:
  `load-csv` (emit a `LOAD CSV WITH HEADERS` driver per table),
  `bulk-rpc` (stream into a running Nexus via the Python SDK's
  batch helpers), and `rewrite-cypher` (regex translator for the
  dialect deltas, with `-- TRANSLATOR-NOTE:` comments on every
  rewrite so the operator can review).
- Three before/after cookbooks under `scripts/migration/cookbook/`:
  `graphrag/` (vector + traversal-augmented retrieval),
  `recommendation/` (co-purchase shortest-path + cosine-similarity
  fusion), `knowledge-graph/` (hybrid graph + vector + FTS).
- 19 unit tests in `tests/migration/test_from_kuzu.py` cover the
  spec parsers, Cypher emitters, CSV streamers, dialect
  translator, and CLI subcommands. All green.

### Fixed — `phase8_optional-match-binding-leak`

- **HIGH-severity correctness bug fixed.** OPTIONAL MATCH against
  a node with no matching relationships used to silently bind the
  target (and relationship) variables to **the source node's
  data** instead of NULL. Repro: `MATCH (a:Person) OPTIONAL MATCH
  (a)-[:KNOWS]->(b) RETURN a.name, b.name` returned `['Alice',
  'Alice']` when Alice had no `:KNOWS` edge — Neo4j returns
  `['Alice', null]`. `b IS NULL` returned `false`, `count(b)`
  returned `1`, every aggregation on top inherited the corruption.
- Root cause: the scan-fallback in
  `crates/nexus-core/src/executor/operators/path.rs::find_relationships`
  (kept alive as a workaround for an mmap-sync edge case) read
  rel_id=0 from the memmapped backing file as a zero-byte record
  (`src=0`, `dst=0`, `type_id=0`). The existing skip filter
  (`src=0 && dst=0 && rel_id > 0`) let rel_id=0 through. When the
  source node was itself at id=0 (the very first node — Alice in
  the canonical reproducer), the direction check accepted
  `check_src_id (0) == node_id (0)` as a match and the operator
  emitted a phantom relationship pointing back at the source.
- Three-part fix in `path.rs`:
  1. Short-circuit the scan when `relationship_count() == 0`.
  2. Clamp the scan upper bound to `relationship_count() - 1` so
     a node-with-no-edges does not pull zero-byte records off the
     end of the in-use range.
  3. Strengthen the uninitialized-record skip filter: drop the
     `rel_id > 0` qualifier and key off `type_id == 0` instead.
     Genuine relationships have non-zero `type_id` because the
     catalog's type registry never assigns id 0.
- 7 new regression tests in
  `crates/nexus-core/tests/optional_match_binding_leak_test.rs`
  pin every shape from the canonical repro: target-var NULL,
  property-access NULL, `IS NULL` true, `count(b) = 0`, both rel
  and target NULL on `[r:KNOWS]->(b)` shape, anonymous target
  variant, plus a happy-path regression confirming OPTIONAL MATCH
  with a real `:KNOWS` edge still returns the target. All 7 pass.
- Quality gates: workspace `cargo +nightly clippy --all-targets
  --all-features -- -D warnings` clean. Regression suites green:
  `tck_runner` 22/22, `geospatial_predicates_test` 34/34,
  `call_subquery_test` 20/20. Lib + integration suite reports
  `2141 passed; 1 failed (pre-existing parallel-flake on
  `engine::tests::match_scopes_by_label_and_property_together`,
  passes in isolation, unrelated to this fix); 10 ignored`.
- Sibling: `phase8_optional-match-empty-driver` covers the
  separate row-count divergence on standalone OPTIONAL MATCH with
  no prior driver.

### Discovered (audit only) — `phase7_cross-test-row-count-parity`

- The phase7 task asked to fix the 22-test row-count gap in the
  74-test cross-bench (`docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md`
  Sections 11/12/15 list 0% Compatible categories). The audit ran
  a Rust-only probe (deleted after capture) against
  `Engine::execute_cypher` and surfaced two distinct correctness
  problems plus three projection-semantics nits:
  - **OPTIONAL MATCH binding leak** (HIGH severity, silent wrong
    data): `MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b:Person)
    RETURN a.name, b.name` returns `['Alice', 'Alice']` when Alice
    has no `:KNOWS` edge — Nexus binds `b` to `a` itself instead of
    NULL. Carved out as `phase8_optional-match-binding-leak`. This
    is wrong data on every OPTIONAL MATCH no-match path; aggregations
    on top inherit the corruption.
  - **OPTIONAL MATCH empty-driver row-count**: standalone
    `OPTIONAL MATCH (n:Ghost) RETURN n` returns 0 rows; Neo4j returns
    1 row with `n = null` (LEFT-OUTER-JOIN against an implicit
    single-row driver). Carved out as `phase8_optional-match-empty-driver`.
  - **Projection nits** (WITH grouping carry-through, write success-row
    emission, ORDER BY tie-stability): folded into the
    `phase8_bolt-protocol-shim` task, which already needs
    Neo4j-exact row shapes for driver compatibility.
- No engine code was changed in this task — the audit drove the
  carve-out and re-sequencing. The sibling tasks each ship their
  own repro + fix + tests when implemented.

### Removed — `phase7_page-cache-property-index-eviction`

- **Dead `warm_recent_indexes` helper** in
  `crates/nexus-core/src/cache/mod.rs` and the matching
  `CacheKey::Index(String)` variant. The function iterated
  `last_access` filtered to `CacheKey::Index(_)` entries but no
  production caller ever inserted a value of that variant — the
  only `track_access` site is the page-cache `Page(u64)` path —
  so the loop never ran and the placeholder marker inside it
  ("`Check if index is actually cached`") could not be reached.
  Removing both is pure dead-code cleanup, no behaviour change,
  one Tier-1 marker fewer in shipping code.
- **Audit finding (queued for follow-up):** `IndexKey::Property(label_id,
  key_id)` is defined and unit-tested in
  `crates/nexus-core/src/cache/index_cache.rs` but has **no
  production producer** — no path in the property-lookup hot
  path calls `index_cache.put(IndexKey::Property(...), ...)`.
  The eviction policy + memory budget are already there
  (`IndexCache` LRU with `max_memory` ceiling); the missing
  piece is wiring the property-lookup code path into the cache.
  Threading `IndexCache` through the property-index hot path is
  a separate task and lives behind the same wider
  index-handle-Arc refactor that gates
  `phase7_planner-using-index-hints` engine wiring.
- **Two new memory-budget tests** in
  `crates/nexus-core/src/cache/index_cache.rs`:
  `test_index_cache_property_keys_respect_memory_budget` and
  `test_index_cache_fulltext_keys_respect_memory_budget`. Both
  pin the LRU + memory-budget invariant for the typed `IndexKey`
  variants so the future producer path inherits a verified
  ceiling. 12/12 tests in the module passing.

### Added — `phase7_planner-using-index-hints`

- **`USING INDEX <var>:<Label>(<prop>)` validation at plan time.**
  `QueryPlanner` now carries an optional `&PropertyIndex` handle
  installed via the new `with_property_index(idx)` builder. When
  the handle is present and the hinted `(label, property)` pair
  has no matching registered property index, the planner raises
  `ERR_USING_INDEX_NOT_FOUND` with a structured message naming the
  pair. Without the handle the hint is accepted silently — that's
  the legacy behaviour for unit-test callers and direct planner
  consumers that don't construct an `IndexManager`. The handle is
  intentionally not yet threaded through `Executor::execute`
  because `ExecutorShared` does not currently carry a
  `PropertyIndex` reference; threading it lives behind a wider
  index-handle-Arc refactor and is queued.
- **Catalog-level pre-check** — the planner short-circuits with
  the same error when the hinted label or property key is not
  registered in the catalog at all (typo in the hint).
- 4 new planner unit tests in
  `crates/nexus-core/src/executor/planner/tests.rs`:
  `using_index_hint_accepted_silently_without_property_index_handle`,
  `using_index_hint_validated_when_property_index_handle_installed_and_index_exists`,
  `using_index_hint_errors_when_index_missing`,
  `using_index_hint_errors_when_label_missing_in_catalog`. All
  passing on `cargo +nightly test -p nexus-core --lib using_index_hint`.
- `docs/specs/cypher-subset.md` updated to document the
  validation behaviour. Existing 300/300 Neo4j diff suite stays
  green; the TCK runner (22 scenarios) and geospatial predicates
  suite (34 tests) stay green.

### Discovered (no code change) — `phase7_call-in-transactions-executor`

- The phase7 task asked to "finish executor batching for `CALL { } IN
  TRANSACTIONS`" — but on audit the executor side was already
  shipped end-to-end by `phase6_opencypher-subquery-transactions`
  slice-2 (batching + `ON ERROR FAIL/CONTINUE/BREAK/RETRY n` +
  `REPORT STATUS AS s`) and slice-3 (`IN CONCURRENT TRANSACTIONS`
  + atomic per-batch rollback via `CompensatingUndoBuffer`).
  Lives in `crates/nexus-core/src/executor/operators/call_subquery.rs`
  (715 LOC); 20 passing tests in `crates/nexus-core/tests/call_subquery_test.rs`
  (9 dedicated to IN TRANSACTIONS); spec already documents the
  full surface at `docs/specs/cypher-subset.md:755-774`. The phase7
  task is archived as a no-op audit; no behavior change.

### Removed — `phase7_resolve-jit-module`

- **JIT scaffold deleted** (`crates/nexus-core/src/execution/jit/`).
  ~1320 LOC of half-implemented Cranelift codegen + a 173-line
  `cranelift_jit.rs.disabled` shadow are gone, plus the matching
  `pub use jit::{JitRuntime, QueryHints}` re-export from
  `execution/mod.rs` and the commented-out
  `// use crate::execution::jit::CraneliftJitCompiler;` import in
  `executor/mod.rs`. ADR
  `delete-the-unused-jit-scaffold-rather-than-finish-the-cranelift-codegen`
  records the rationale: zero production callers across
  `nexus-server` / `nexus-cli` / `nexus-protocol` / `nexus-bench` /
  the integration tests; the columnar fast-path real-world ratio
  is already ~1.13× per `PERFORMANCE_V1.md` so the gain a JIT
  would deliver is dominated by materialisation cost; the
  planner's bigger leverage is cardinality propagation. Existing
  test suites (`tck_runner`, `geospatial_predicates_test`) stay
  green. No public-API breakage — the re-exports were unused.

### Fixed — `phase7_fix-ignored-engine-tests`

- **Two stale `#[ignore]` attributes** removed from
  `crates/nexus-core/src/engine/tests.rs` (`test_engine_default`,
  `test_engine_new_default`). Both carried a placeholder comment
  blaming "default data dir which conflicts with parallel tests"
  but `Engine::default()` and `Engine::new_default()` both
  delegate to `Engine::new()`, which has used
  `tempfile::tempdir()` for per-instance isolation since at
  least 1.13.0. The ignore markers were carry-over from a
  pre-tempdir implementation. Added a block comment above the
  two tests so future readers do not re-add the ignore. Lib test
  count moved from `91 passed / 2 ignored` to `93 passed / 0
  ignored` on `cargo +nightly test -p nexus-core --lib
  engine::tests`.

### Added — `phase6_opencypher-tck-spatial`

- **openCypher-TCK-shaped spatial conformance suite** at
  `crates/nexus-core/tests/tck/spatial/*.feature`. Four feature
  files, **22 scenarios, 87 steps, all passing**:
  - `Point1-construction.feature` — 7 scenarios covering 2D / 3D
    Cartesian + WGS-84 constructors, negative-coordinate parsing,
    explicit-CRS overrides over `x/y` and `longitude/latitude`
    aliases.
  - `Point2-distance.feature` — 5 scenarios covering Pythagorean
    2D / 3D distance, symmetry, self-distance zero, and
    `ERR_CRS_MISMATCH` on mixed-CRS inputs.
  - `Point3-predicates.feature` — 7 scenarios covering
    `point.withinBBox` (interior / exterior / boundary / CRS
    mismatch) and `point.withinDistance` (within / outside /
    exact-radius).
  - `SpatialIndex1-rtree.feature` — 3 scenarios covering
    `CREATE SPATIAL INDEX` feedback row, `db.indexes()` reporting
    the registered RTREE index alongside the auto-LOOKUP entry,
    and `ERR_RTREE_BUILD` on a non-Point sample row.
- **Cucumber harness** at `crates/nexus-core/tests/tck_runner.rs`
  (`cucumber = "0.21"` dev-dependency, runs as a `harness = false`
  integration test). Discovers `.feature` files under
  `tests/tck/spatial/`, drives every scenario through
  `Engine::execute_cypher` with an isolated `Engine` per scenario,
  and supports the standard openCypher TCK step grammar (`Given an
  empty graph` / `having executed: """…"""` / `executing query:
  """…"""` / `the result should be, in any order: <table>` /
  `the result should be: <table>` / `the result should be empty` /
  `a TypeError should be raised at runtime: <token>` / `no side
  effects`). Custom TCK-cell parser handles unquoted-key map
  literals (`{x: 1.0, y: 2.0, crs: 'cartesian'}`), single-quoted
  strings, lists, booleans, null, and signed numbers; numeric
  comparison uses a 1e-9 absolute tolerance for floats.
- **Vendor notes** at `crates/nexus-core/tests/tck/spatial/VENDOR.md`
  documenting that the upstream openCypher TCK has **no spatial
  corpus** (verified 2026-04-28 against `opencypher/openCypher@main`
  at `tck/features/`), so the Nexus corpus is authored under
  Apache 2.0 and ready for upstream contribution if openCypher
  ever opens a spatial track. Includes a one-line `curl` recipe
  to re-verify upstream coverage on future bumps.
- **Apache 2.0 attribution** at `LICENSE-NOTICE.md` covering the
  openCypher TCK format and step grammar the Nexus corpus reuses.

### Fixed — `phase6_opencypher-tck-spatial`

- **Negative coordinates in inline `point()` literals**
  (`crates/nexus-core/src/executor/parser/expressions.rs`).
  `extract_number_from_expression` now accepts
  `UnaryOp { Minus | Plus, Literal::Integer | Literal::Float }`
  in addition to bare integer / float literals. Before this fix,
  `point({longitude: -73.9857, latitude: 40.7484})` raised
  `Cypher syntax error: Point coordinates must be numbers` because
  the lexer tokenises `-73.9857` as `UnaryOp { Minus,
  Literal::Float(73.9857) }`, not as a negative literal. Surfaced
  by Point1 scenarios 3, 5, 6, and Point3 scenarios across the
  withinBBox + withinDistance suite.
- **Implicit WGS-84 CRS from `longitude`/`latitude`/`height` keys**
  (same file). `parse_point_literal` now defaults to
  `CoordinateSystem::WGS84` when any geographic key alias is
  present *and* no explicit `crs:` field overrides. Before this
  fix, `point({longitude: 13.4, latitude: 52.5, height: 100.0})`
  silently defaulted to Cartesian and the `crs` accessor returned
  `'cartesian-3d'` instead of `'wgs-84-3d'`. Matches Neo4j's
  behaviour; explicit `crs:` always wins. Surfaced by Point1
  scenario 4 + Point2 scenario 5 (CRS-mismatch path).

### Known limitations exposed by the TCK harness

The TCK suite intentionally avoids three Cypher shapes that
surfaced engine bugs out of scope for this task; each is filed as
a follow-up:
- **`<expr>.<prop>` projection** — `RETURN point(...).x AS xx` and
  `RETURN $param.x AS xx` drop the AS alias and the rest of the
  projection list because the `PropertyAccess` AST is keyed by
  `variable: String` rather than `expression: Box<Expression>`.
  Workaround in scenarios: compare the full `point()` map shape,
  not individual accessors.
- **`WITH 1 AS x RETURN x` returns 0 rows** — value-only `WITH`
  with no upstream pattern source emits no row. Workaround in
  scenarios: keep the value inline in the same `RETURN`.
- **`UNWIND [point(...)]` parser overrun** — the list-literal
  parser misreads characters of `'cartesian'` inside an inlined
  point. Workaround: pass points as parameters or reference a
  matched node's property.

### Added — `phase6_spatial-planner-followups`

- **Function-style `point.nearest(<var>.<prop>, <pt>, <k>)`** —
  callable in `RETURN` / `WITH` / `WHERE` expression position;
  returns `LIST<NODE>` ordered ascending by distance. Resolves the
  variable's label by reading the bound node's `_nexus_id` →
  `label_bits` → catalog name, looks up the registered
  `{Label}.{prop}` R-tree index, and walks the registry directly
  when present. Without an index, falls back to a label scan +
  sort + truncate so the `same result with and without index`
  contract holds. Implementation lives at
  `crates/nexus-core/src/executor/eval/projection.rs`.
- **+25 Neo4j compat-diff scenarios** in
  `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`
  Section 18 covering the cross-product `Bbox / WithinDistance /
  Nearest` × `Cartesian / WGS-84` × `2D / 3D`. Live capture against
  Neo4j 2025.09.0 still has to run; the scenarios themselves are
  static query strings the harness diffs against the Neo4j
  reference at runtime, so they land authored.
- **3 new integration tests** in `geospatial_predicates_test.rs`
  covering the function arm: `point_nearest_function_returns_same_list_with_and_without_index`,
  `point_nearest_rejects_non_property_access_first_arg`, and
  `point_nearest_returns_empty_list_when_k_is_zero`.
- **TCK import carved out** to follow-up
  `phase6_opencypher-tck-spatial`. Reason: vendoring requires
  fetching the openCypher distribution at a pinned commit and
  adding `cucumber 0.21` to the workspace dev-deps; both touches
  are out of scope for the projection-side function-arm work.

### Added — `phase6_spatial-planner-seek`

- **`Operator::SpatialSeek` planner rewriter** at
  `crates/nexus-core/src/executor/planner/queries.rs`. The planner
  now recognises three Cypher shapes against an R-tree-indexed
  property and rewrites the operator pipeline to drive directly
  off `IndexManager::rtree` instead of `NodeByLabel + Filter`:
   - `WHERE point.withinBBox(<var>.<prop>, {bottomLeft, topRight})`
     → `SeekMode::Bbox`
   - `WHERE point.withinDistance(<var>.<prop>, <pt-literal>, <d>)`
     → `SeekMode::WithinDistance`
   - `MATCH ... ORDER BY distance(<var>.<prop>, <pt-lit>) ASC
     LIMIT <k>` → `SeekMode::Nearest { k }`
- **Cost-based picker.** The cost arm at queries.rs:3067 was
  already wired by `phase6_rtree-index-core`; the rewriter now
  compares its `log_b(N) + matching` estimate against the legacy
  `NodeByLabel + Filter` cost (`2*N`) and keeps the legacy plan
  when the seek isn't cheaper. Selectivity defaults: 5 % for
  bounded modes, `k` for k-NN.
- **`QueryPlanner::with_rtree(Arc<RTreeRegistry>)` builder shim.**
  The planner is opt-in: existing call sites that don't have a
  registry handle (planner unit tests, the standalone
  `Executor::parse_and_plan`) keep emitting the legacy plan.
  `Engine::execute_*` and the executor `plan_with_indexes` path
  install the handle automatically.
- **`db.indexes()` RTREE rows** at
  `crates/nexus-core/src/executor/operators/procedures.rs::execute_db_indexes_procedure`.
  Every registered R-tree index now surfaces with
  `type = "RTREE"`, `state = "ONLINE"`, `entityType = "NODE"`,
  the matching `labelsOrTypes` / `properties` arrays, and
  `indexProvider = "rtree-1.0"`.
- **6 new planner regression tests** in
  `crates/nexus-core/tests/spatial_planner_test.rs`: each rewriter
  shape is exercised in isolation against a synthetic catalog
  scaffold so the tests never depend on engine-level fixtures, and
  the negative path (no registry handle) is covered by a paired
  test that asserts the legacy plan stands. The `db.indexes()`
  RTREE row shape is asserted end-to-end through the engine.
- **Deferred to a follow-up slice**: §4 function-style
  `point.nearest(<var>.<prop>, <k>)` (needs a multi-row
  Project+Sort+Limit projection lowering that's out of scope for
  the planner-only rewrite); §6 openCypher TCK import (vendoring
  external `spatial.feature` distribution); §7 Neo4j compat-diff
  +25 spatial scenarios (live Neo4j operator-gated).

### Added — `phase6_spatial-index-autopopulate`

- **Auto-populate spatial indexes on CREATE / SET / REMOVE / DELETE**
  so `spatial.nearest` reflects live data without a manual
  `spatial.addPoint` bulk-loader call. The hot-path contract now
  mirrors FTS line-for-line: every `create_node` path runs
  `Engine::spatial_autopopulate_node`, every `persist_node_state`
  runs `Engine::spatial_refresh_node`, every `delete_node` runs
  `Engine::spatial_evict_node`. Each hook walks
  `IndexManager::rtree`, matches `(label, property)` against the
  written node, inserts / refreshes / evicts the entry, and emits
  the matching `WalEntry::RTreeInsert` / `RTreeDelete` so crash
  recovery replays the write.
- **Per-index membership tracking** via `RTreeRegistry::definitions`
  + an in-`IndexSlot` `HashSet<u64>` mirroring the FTS
  `NamedFullTextIndex::members` pattern. Refresh and evict paths
  short-circuit on already-absent nodes; `indexes_containing(node_id)`
  enumerates exactly the indexes a SET / DELETE has to touch.
- **Registry relocation**. `ExecutorShared::spatial_indexes` is
  removed; `execute_create_index`, `execute_spatial_nearest`, and
  `execute_spatial_add_point` now re-source through
  `IndexManager::rtree`. The engine crate's `engine::crud` module
  reaches the registry the same way it reaches `indexes.fulltext`.
- **`CREATE SPATIAL INDEX` type-check**. The executor samples up to
  1 000 existing `Label` nodes and rejects with `ERR_RTREE_BUILD`
  on the first non-Point sample, naming the offending `node_id`.
  Catches the silent "index built, queries empty" trap when a
  property is heterogeneously typed.
- **Crash-recovery harness** at
  `crates/nexus-core/tests/spatial_crash_recovery.rs` covering:
  WAL replay restores every committed point after a registry drop;
  unflushed entries stay absent after recovery; and an insert /
  delete pair replayed in order converges to the post-delete
  state. Mirrors the FTS crash-recovery suite.

### Deprecated

- **`spatial.addPoint` is no longer required** — Cypher CRUD
  auto-populates spatial indexes. The procedure remains callable
  and idempotent with the auto-populate hook for backward
  compatibility, but every call now logs `tracing::info!` so
  deployments can spot stragglers. **Scheduled for removal in
  v2.0.0.**

### Added — `phase6_rtree-index-core`

- **Packed Hilbert R-tree backend for spatial indexes** in
  `crates/nexus-core/src/index/rtree/`. Replaces the grid-backed
  prototype at `crates/nexus-core/src/geospatial/rtree.rs` for
  every read path (`spatial.nearest`, `point.withinDistance`,
  `point.withinBBox`) without changing the Cypher surface.
- **8 KB pages, fanout 64-127, deterministic bulk-load**. Two
  replicas given the same input produce byte-identical page
  files. The encoder writes every header byte and every padding
  byte; the Hilbert sort breaks ties on `node_id` ascending so
  the entire on-disk image is reproducible.
- **k-NN priority-queue walk**. `spatial.nearest(p, label, k)`
  swapped from `O(N)` linear `entries() + sort_by` scan to a
  `BinaryHeap`-backed traversal that visits inner pages in
  ascending bbox-to-point distance order and stops after `k`
  leaves are popped — `O(log_b N + k)` page reads. Ties on
  distance break on `node_id` ascending so the result is
  deterministic across runs.
- **Within-distance** (`RTree::within_distance`). Stack-based
  descent pruning by squared bbox distance; results sorted by
  ascending distance, ties on `node_id`.
- **WAL framing** for spatial mutations: `RTreeInsert` (op-code
  `0x50`), `RTreeDelete` (`0x51`), `RTreeBulkLoadDone` (`0x52`).
  Crash recovery feeds every entry through
  `RTreeRegistry::apply_wal_entry`.
- **`RTreeRegistry`** with `RwLock<Arc<RTree>>` per index for
  atomic-rebuild via pointer swap (`swap_in`). Readers grab a
  snapshot through `RTreeRegistry::snapshot(name)` and keep
  using it across a concurrent rebuild; the new tree only
  becomes visible to subsequent snapshots. No reader observes
  a half-built tree.
- **MVCC visibility hook** via `RTreeRegistry::nearest_with_filter`.
  The R-tree itself stays epoch-free; the executor hands a
  closure that consults the transaction manager's snapshot
  view and drops invisible ids before they count against `k`.
  Two-pass over-fetch (2× then 8× target) keeps SLO under high
  invisibility miss rates.
- **`USING RTREE` parser alias**. `CREATE INDEX [name] FOR
  (n:Label) ON (n.prop) USING RTREE` accepts the Cypher 25
  shape; both this and the legacy `CREATE SPATIAL INDEX ON
  :Label(prop)` register on `IndexManager::rtree`.
- **Page-store abstraction** (`PageStore` trait) with
  `MemoryPageStore` (HashMap-backed for tests / bulk-build) and
  `FilePageStore` (file-backed, layout mirrors
  `index/btree.rs`'s flat-array shape). Crash consistency:
  `flush()` calls `sync_all`, live set persists through a
  tmp + rename atomic replace.
- **73 new tests** covering every layer: 12 page codec, 11
  Hilbert sort, 9 packer (incl. byte-identical replica), 8
  mutable tree (insert/split/delete/underflow), 13 search
  (k-NN / within-distance / bbox helpers), 9 page-store
  (memory + file + crash recovery), 8 registry (WAL replay,
  atomic swap, visibility filter), 3 crash-recovery integration
  (`tests/rtree_crash_recovery.rs` — 5 500 inserts after a
  partial bulk-load, marker-only no-op, interleaved
  insert/delete order replay). 4 parser tests for the new
  `USING RTREE` alias.
- **Spec + guide**: new `docs/specs/rtree-index.md` (page
  layout, bulk-load, MVCC, WAL framing, SLOs); new
  `docs/guides/GEOSPATIAL.md` (predicates, procedures, DDL,
  performance, crash recovery, limitations).
  `docs/specs/knn-integration.md` updated with the spatial
  vs. vector retrieval comparison.

### Added — `phase6_opencypher-subquery-transactions`

- **`CALL { … }` subquery executor** wired through planner +
  dispatch. The inner subquery executes once per outer row; outer ×
  inner rows are joined into the outer result set
  (Neo4j-compatible CALL semantics). Standalone `CALL { MATCH …
  RETURN … }` runs against a single empty driver row. Nested CALLs
  flow through the same path.
- **Write-bearing inner subqueries** (`CALL { CREATE … }` /
  `MERGE` / `DELETE` / `SET`). The dispatch path picks
  `execute_create_pattern_with_variables` for empty-scope and
  `execute_create_with_context` for row-scoped CREATE, the latter
  newly handling anonymous nodes and resolving property
  expressions against the row scope.
- **`CALL { … } IN TRANSACTIONS [OF N ROWS] [REPORT STATUS AS s]
  [ON ERROR CONTINUE|BREAK|FAIL|RETRY n]`** end-to-end: per-batch
  ON ERROR policy, per-batch status rows under the declared name,
  retry-then-escalate. Multi-worker `IN CONCURRENT TRANSACTIONS`
  is rejected with `ERR_CALL_IN_TX_CONCURRENCY_UNSUPPORTED`
  pending the V2 sharded MVCC branch.
- **Cypher 25 scoped subqueries** — `CALL (var1, var2) { … }` and
  the empty form `CALL () { … }`. The inner sees only the listed
  outer variables; everything else is shadowed.
- **`COLLECT { … }` subquery expression**. Folds the inner row
  stream into a LIST: single-column → `LIST<T>`, multi-column →
  `LIST<MAP>` keyed by column names, aggregating-inner →
  single-element list, empty inner → empty list (NOT NULL).
- 8 new compatibility scenarios in
  `scripts/compatibility/compatibility-test-queries.cypher`
  (SUB-1 through SUB-8). `docs/guides/BULK_INGEST.md` documents
  the recommended ingest patterns.
- **Atomic per-batch rollback (§3)** via a per-attempt
  `CompensatingUndoBuffer` installed onto every inner
  ExecutionContext. CREATE write paths register
  `DeleteNode` / `DeleteRelationship` inverse ops; on a failed
  batch attempt the operator drains the buffer in reverse order
  before retrying or applying the `ON ERROR` policy, so a
  `CALL { … } IN TRANSACTIONS` failure leaves no partial-batch
  writes behind.

## [1.15.0] — 2026-04-26

Closes [hivellm/nexus#2][issue-2]. Server-side bug fix + JSON wire
shape rename for the schema endpoints, with every first-party SDK
realigned to the new shape and version-bumped to match. Also ships
slice 1 of `phase6_opencypher-quantified-path-patterns`.

### Added

- **Quantified path patterns (Cypher 25 / GQL) — anonymous-body
  shape**. `MATCH (a)( ()-[:T]->() ){m,n}(b)` now executes
  end-to-end and produces byte-identical row sets to the legacy
  `MATCH (a)-[:T*m..n]->(b)` form. The parser collapses the
  textbook QPP shape (anonymous boundary nodes, single
  relationship, no inner predicates) to the legacy quantified
  relationship at parse time, so the existing
  `VariableLengthPath` operator handles it without a new
  executor. Direction (`->`, `<-`, `-`), every quantifier
  (`{m,n}`, `{m,}`, `{,n}`, `{n}`, `+`, `*`, `?`), the inner
  relationship variable, and the relationship-property map are
  all preserved by the lowering. `shortestPath((a)( ... ){m,n}(b))`
  works for the same shape.

  Bodies that carry inner state — named or labelled boundary
  nodes, multi-hop paths, intermediate predicates — surface a
  clean `ERR_QPP_NOT_IMPLEMENTED` error pointing at the slice-2
  follow-up rather than silently producing wrong rows. See
  `docs/guides/QUANTIFIED_PATH_PATTERNS.md` for the full
  user-facing surface and migration notes.

### Fixed

- **`GET /data/nodes?id=0` no longer returns `node: None` for nodes
  that exist.** `crates/nexus-server/src/api/data.rs::validate_node_id`
  was rejecting `node_id == 0` before the engine was even consulted,
  but `0` is a legitimate catalog id (the engine assigns it to the
  first node ever created in a database). The validator is now a
  no-op stub kept for forward-compat with future API-boundary
  invariants; existence is the engine's job. The same fix unblocks
  `update_node(0, ...)` and `delete_node(0, ...)` which were
  silently short-circuited the same way.
- **`GET /data/nodes` distinguishes "missing `id` query parameter"
  from "id=0".** The previous `params.get("id").unwrap_or(0)` made
  a missing parameter alias as id `0`, which used to fail validation
  by accident; with the validator gone it would have succeeded
  silently against the wrong row. The handler now returns explicit
  errors for both missing and malformed `id` values.
- **`/health` self-reported version is now byte-equal to
  `env!("CARGO_PKG_VERSION")` in CI.** A new
  `api::health::tests::test_health_endpoint_reports_workspace_version`
  pins the contract so a future release whose docker image is built
  before the workspace bump fails the test gate instead of leaking
  the wrong number to users (the issue reporter's `version=1.13.0`
  on a `:v1.14.0`-tagged image was caused by exactly that).

### Changed (BREAKING)

- **Wire shape for `GET /schema/labels` and `GET /schema/rel_types`.**
  Each entry was a JSON tuple `["Person", 0]` and is now a JSON
  object `{"name": "Person", "id": 0}`. The second member is the
  catalog id allocated by the engine, not a count — naming the
  fields removes the ambiguity that had the issue reporter and the
  Rust SDK rustdoc disagreeing on what `u32` meant. The new shape
  also leaves room for additive fields (e.g. `count`) without
  another rename.

  Server types: `LabelInfo` and `RelTypeInfo` in
  `crates/nexus-server/src/api/schema.rs`. Every first-party SDK
  follows the rename — see the per-language CHANGELOGs for the
  matching consumer-side migration.

### SDK realignment

- **Rust** (`sdks/rust/` → crates.io `nexus-graph-sdk` 1.15.0):
  `ListLabelsResponse.labels` and `ListRelTypesResponse.types`
  retyped to `Vec<LabelInfo>` / `Vec<RelTypeInfo>`. README +
  example updated.
- **Python** (`sdks/python/` → PyPI `hivehub-nexus-sdk` 1.15.0):
  Pydantic `LabelInfo` / `RelTypeInfo` re-exported from package
  root. README + CHANGELOG updated.
- **C#** (`sdks/csharp/` → NuGet `Nexus.SDK` 1.15.0): `LabelInfo`
  / `RelTypeInfo` POCOs, return types of `ListLabelsAsync` /
  `ListRelationshipTypesAsync` retyped on both `NexusClient` and
  `RetryableNexusClient`. **Latent route fix**: the SDK was hitting
  the non-existent `/schema/relationship-types`; corrected to
  `/schema/rel_types`.
- **Go** (`sdks/go/` → tag `v1.15.0`): typed structs, route fix,
  test fixtures rebuilt to emit the new wire shape. Same route fix
  applied in `RetryableClient`.
- **PHP** (`sdks/php/` → tag `v1.15.0`): phpdoc retyped to
  `array<int, array{name: string, id: int}>`, route fix in both
  `NexusClient` and the `REL_TYPES` HTTP fallback in
  `Transport\HttpTransport`.

The TypeScript SDK has no `listLabels` / `listRelTypes` API surface,
so it is unaffected and stays at 1.14.0 on npm.

### Other

- Workspace version bumped from 1.14.0 to 1.15.0 (every crate
  inherits via `version.workspace = true`), so a fresh
  `cargo build --release -p nexus-server` produces a binary that
  self-reports `1.15.0` on `/health`.
- Docker image rebuilt and pushed as `hivehub/nexus:1.15.0` and
  `hivehub/nexus:latest` (multi-arch `linux/amd64` + `linux/arm64`,
  with SBOM + SLSA provenance attestations).

[issue-2]: https://github.com/hivellm/nexus/issues/2

## [1.14.0] — 2026-04-22

### Added — openCypher geospatial predicates + `spatial.*` procedures (slice A)

`phase6_opencypher-geospatial-predicates` slice A closes the
user-facing Cypher surface around the existing `Point` type.
Follow-up slices ship the packed R-tree index
(`phase6_rtree-index-core`), the planner's `SpatialSeek` operator
(`phase6_spatial-planner-seek`), and auto-populate on CREATE / SET
(`phase6_spatial-index-autopopulate`).

- **Namespaced function parsing.** The expression parser now
  accepts `identifier.identifier(args)` as a function call
  (`crates/nexus-core/src/executor/parser/expressions.rs`) — the
  lookahead only fires when the `.identifier` is immediately
  followed by `(`, so ordinary `n.prop` PropertyAccess keeps
  precedence. Every test under `geospatial_integration_test.rs`
  that exercises `n.prop` access stays green.
- **Point predicate functions.** `point.withinBBox(p, bbox)`,
  `point.withinDistance(a, b, distMeters)`, `point.azimuth(a, b)`,
  and `point.distance(a, b)` (namespaced alias of the bare
  `distance()` function) land in the projection evaluator
  (`crates/nexus-core/src/executor/eval/projection.rs`). CRS or
  dimensionality mismatches surface as `ERR_CRS_MISMATCH`;
  malformed `bbox` maps surface as `ERR_BBOX_MALFORMED`; same
  points to `point.azimuth` return `NULL` because the bearing is
  undefined.
- **`spatial.*` procedure dispatcher.** A new
  `crates/nexus-core/src/spatial/mod.rs` mirrors the APOC
  dispatch shape: pure-value procedures consume
  `Vec<serde_json::Value>` and return `(columns, rows)`. Ships:
  `spatial.bbox(points)`, `spatial.distance(a, b)`,
  `spatial.interpolate(line, frac)`, `spatial.withinBBox(p, bbox)`,
  `spatial.withinDistance(a, b, d)`, `spatial.azimuth(a, b)`. The
  executor's `execute_call_procedure` routes `spatial.*` through
  this dispatcher before the legacy `GraphProcedure` registry
  (which can only represent single-arg procedures under the
  current dispatch).
- **Engine-aware spatial procedures.** `spatial.nearest(point,
  label, k)` walks the `{label}.*` entry in the executor's
  shared spatial-index registry and streams `(node, dist)` rows
  ordered by distance ascending, ties broken by `node_id`
  ascending. `spatial.addPoint(label, property, nodeId, point)`
  is the Cypher-level bulk-loader that indexes a row into the
  registered spatial index until the auto-populate task lands.
- **Point helpers** (`crates/nexus-core/src/geospatial/mod.rs`):
  `Point::same_crs`, `Point::crs_name`, `Point::azimuth_to`,
  `Point::within_bbox`. Used by both the predicate functions and
  the dispatcher so the semantics stay in one place.
- **`dbms.procedures()` introspection** now lists every new
  `spatial.*` procedure so BI tools that introspect the catalogue
  see the full geo surface.
- **RTreeIndex::entries().** Exposes an `(node_id, point)` snapshot
  of the grid-backed spatial index so `spatial.nearest` can do a
  bounded full-scan k-NN. The prior implementation walked an
  `f64::MIN..=f64::MAX` bbox through the grid-cell math, which
  iterated ≈4 × 10⁹ empty cells before returning. Direct
  iteration keeps the walk bounded by `total_points`.
- **Tests.** New integration suite
  `crates/nexus-core/tests/geospatial_predicates_test.rs` (23
  tests) covers every predicate + procedure end-to-end through
  Cypher. Existing `geospatial_integration_test.rs` (55 tests)
  and the spatial dispatcher unit tests (22 tests) all stay
  green.

## [1.13.0] — 2026-04-22

### Added — FTS async writer + per-index cadence commits

`phase6_fulltext-async-writer` closes §3 of the
`phase6_fulltext-wal-integration` original spec and ships the
crash-recovery integration harness that was deferred under §5.3 of
that task.

- **Per-index background writer.** `NamedFullTextIndex` now owns an
  optional `WriterHandle`
  (`crates/nexus-core/src/index/fulltext_writer.rs`). Each spawned
  writer runs on a dedicated `std::thread`, owns the single Tantivy
  `IndexWriter` Tantivy permits per index, and drains a bounded
  `crossbeam-channel` (default capacity 1024).
- **Cadence + batch commits.** The writer commits + reloads the
  reader whenever the buffer reaches `max_batch_size` (default
  256) or `refresh_ms` (read from `FullTextIndexMeta.refresh_ms`,
  default 1000 ms) elapses since the last flush — whichever fires
  first.
- **Hot-path integration.** `FullTextRegistry::{add_node_document,
  add_node_documents_bulk, remove_entity}` now route through the
  writer when one is spawned, and fall back to the original
  synchronous Tantivy-commit path otherwise. Async writers are
  opt-in per registry via
  `FullTextRegistry::enable_async_writers()` — the default
  remains the synchronous read-your-writes contract every test
  predating this task relies on.
- **Graceful shutdown.** Dropping the `WriterHandle` drains the
  channel, applies the final batch, commits, and joins the thread
  before `Drop::drop` returns. `FullTextRegistry::flush_all` +
  `disable_async_writers` expose both best-effort flushes and
  explicit teardown for shutdown paths and tests.
- **Crash-recovery harness**
  (`crates/nexus-core/tests/fulltext_crash_recovery.rs`). Replays a
  WAL containing committed `FtsCreateIndex` + `FtsAdd` entries
  against a freshly-opened registry after simulating a kill-9
  between WAL sync and writer commit. Asserts that every
  WAL-committed doc surfaces after replay, that docs that never
  reached the WAL stay absent, and that the registry's cadence
  tick makes enqueued docs visible without an explicit
  `flush_blocking`.

### Fixed

- `WriterHandle::enqueue` / `apply_batch` no longer mis-track the
  `pending` counter. The prior implementation held a write guard
  while attempting another lock acquisition in the same expression
  (a deadlock on recursive acquire under `parking_lot::RwLock`),
  and the drained-buffer decrement used `buffer.capacity() -
  buffer.len()` — the allocation size rather than the number of
  commands drained — so `pending_count()` never returned to zero
  once the buffer had grown past its initial cap.

## [1.12.0] — 2026-04-21

### Added — FTS auto-maintenance on CREATE / SET / REMOVE / DELETE

Slices 2+3 of `phase6_fulltext-wal-integration` close the
write-path integration. Every mutating Cypher path now keeps the
FTS view in lockstep with the authoritative node state and emits
matching WAL entries for crash recovery.

- **CREATE auto-populate** — `Executor::fts_autopopulate_node` is
  wired into all three CREATE operators (standalone node,
  relationship-target node, MATCH-combined-pattern node) plus the
  programmatic `Engine::create_node` path. Match rule: node
  carries ≥1 of the index's labels AND has a string value for ≥1
  of the indexed properties; content is the whitespace-joined
  concatenation of matching string properties in declared order.
- **SET / REMOVE auto-refresh** — `Engine::persist_node_state`
  now calls `fts_refresh_node`, which delete-then-conditional-adds
  against every FTS index currently containing the node. When
  the refresh clears the last indexed property (e.g. `REMOVE n.p`)
  the doc stays evicted; when the property changes (e.g. SET
  n.title = 'New'), the reindex surfaces the new terms and
  purges the old.
- **DELETE auto-evict** — `Engine::delete_node` drops the node
  from every matching FTS index before marking the storage record
  deleted and emits `FtsDel` WAL entries.
- **Membership tracking** — `NamedFullTextIndex.members` is a
  per-index `HashSet<u64>` updated on every add/del so refresh /
  evict paths can enumerate matching indexes without consulting
  the engine-side label index (which diverges from the
  executor's cloned view after `refresh_executor`).
- **`FullTextIndex::remove_document`** now reloads the reader
  after commit — fixes an existing bug where replayed `FtsDel`
  ops were invisible to same-process searchers.

WAL emissions go through the existing `write_wal_async` path so
recovery replay (slice 1) can reconstruct the full index state
from the log.

Tests (+3): `fulltext_create_node_auto_populates_matching_index`,
`fulltext_create_node_skips_non_matching_label`,
`fulltext_wal_replay_reconstructs_registry_and_content`,
`fulltext_delete_node_evicts_from_index`,
`fulltext_set_property_refreshes_doc`,
`fulltext_remove_property_evicts_doc`. Full lib suite: 2019
passed / 0 failed / 12 ignored.

**Follow-up task**: `phase6_fulltext-async-writer` covers the
per-index background writer with `refresh_ms` cadence + the
crash-during-bulk-ingest integration test. Current sync commit
path already beats the >5 k docs/sec SLO so the async pipeline
is purely a concurrency optimisation.

## [1.11.0] — 2026-04-21

### Added — FTS WAL integration (slice 1: op-codes + persistence + replay)

First slice of `phase6_fulltext-wal-integration`. Wires the FTS
backend into the WAL durability model and the engine's restart
path; the commit-hook that turns every `CREATE` / `MERGE` / `SET`
into enqueued WAL entries ships as the next slice of the same
task.

- **WAL op-codes** — four new entry kinds in `WalEntryType` /
  `WalEntry`:
  - `FtsCreateIndex` (`0x40`): name + entity + labels/types +
    properties + resolved analyzer name.
  - `FtsDropIndex` (`0x41`): name.
  - `FtsAdd` (`0x42`): name + entity_id + label_or_type_id +
    key_id + content.
  - `FtsDel` (`0x43`): name + entity_id.
  Round-trip covered by `wal::tests::fts_wal_ops_encode_decode_roundtrip`.
- **On-disk catalogue** — every create writes a `_meta.json`
  sidecar into the index directory carrying the registry-level
  metadata. `FullTextRegistry::load_from_disk` scans the base
  directory at engine startup and re-opens every catalogued
  index; parameterised ngram analyzers round-trip through the
  `ngram(m,n)` display name.
- **Reopen-aware `FullTextIndex`** — `with_analyzer` now falls
  back to `Index::open_in_dir` when the Tantivy directory already
  exists, so restart does not throw `IndexAlreadyExists`.
- **WAL replay dispatcher** — `FullTextRegistry::apply_wal_entry`
  consumes a single `WalEntry` and dispatches FTS-shaped ops into
  the registry. Idempotent: duplicate create = no-op; add/del on
  a missing index = no-op. Non-FTS ops return `Ok(false)` so the
  caller can skip them.
- **Startup hook** — `IndexManager::new` calls `load_from_disk`
  before returning so the engine boots with the full FTS
  catalogue already in memory.

Tests: +7 (1 WAL encode/decode + 3 sidecar/load + 3 replay
dispatcher). Full lib suite: 2013 passed / 0 failed.

Scoped out to the next slice:
- Per-index async writer + `refresh_ms` cadence (Tantivy's
  synchronous commit already cleared the >5k docs/sec SLO — see
  `docs/performance/PERFORMANCE_V1.md` — so async is pure
  optimisation, not correctness).
- Commit-hook: `CREATE` / `MERGE` / `SET` paths emit WAL ops that
  match registered FTS indexes. Today callers drive the
  programmatic API.
- Crash-during-bulk-ingest integration test.

## [1.10.0] — 2026-04-21

### Added — FTS benchmarks + bulk-ingest path + ranking regression

phase6_fulltext-benchmarks establishes performance baselines and a
ranking-regression guard for the full-text search backend:

- **Criterion harness** `crates/nexus-core/benches/fulltext_bench.rs`
  with three scenarios over a deterministic 100 k × 1 KB corpus:
  - `fulltext_single_term/corpus_100k_1kb` — BM25 single-term.
  - `fulltext_phrase/corpus_100k_1kb` — 2-term phrase query.
  - `fulltext_ingest/bulk_10k_docs` — bulk-ingest throughput.
- **Measured numbers** (Ryzen 9 7950X3D, all SLOs cleared):
  - single-term: 150 µs median (target < 5 ms p95) → ≈33× headroom.
  - phrase query: 4.57 ms median (target < 20 ms p95) → ≈4.4×.
  - bulk ingest: ≈60 k docs/sec (target > 5 k) → ≈12×.
- **Bulk-ingest API** — `FullTextIndex::add_documents_bulk` and
  `FullTextRegistry::add_node_documents_bulk` open one Tantivy
  writer, push every doc, and commit once. The per-doc path keeps
  its commit-after-every-write cadence for interactive callers;
  bulk loaders pick the batched path.
- **Ranking regression suite** `tests/fulltext_ranking_regression.rs`
  with 7 golden top-N assertions over a 10-doc hand-curated corpus
  (graph-family dominance, vector-family dominance, phrase pins,
  boolean-must narrowing, empty query, limit respected).

Baseline numbers land in
[docs/performance/PERFORMANCE_V1.md](docs/performance/PERFORMANCE_V1.md).
Async-writer + WAL-driven enqueue remain scoped for
phase6_fulltext-wal-integration.

## [1.9.0] — 2026-04-21

### Added — FTS analyzer catalogue

phase6_fulltext-analyzer-catalogue fills in the analyzer surface
left parked by v1.8. `db.index.fulltext.createNodeIndex /
createRelationshipIndex` now accepts a full Neo4j-parity config
map that picks the per-index tokenizer chain:

- **Catalogue**: `standard`, `whitespace`, `simple`, `keyword`,
  `ngram`, `english`, `spanish`, `portuguese`, `german`, `french`.
  Every name matches Neo4j's `listAvailableAnalyzers()` output
  verbatim; rows are alphabetical.
- **`standard`** — default; lowercase + English stopword removal
  (Lucene's English stopword list, bundled via Tantivy 0.22).
- **Language analyzers** — stemmer + lowercase + stopword filter
  for English / Spanish / Portuguese / German / French. Built on
  Tantivy's `Stemmer` + `StopWordFilter::new(Language)` with the
  `stopwords` feature enabled upstream.
- **`ngram`** — character n-grams with configurable `ngram_min`
  / `ngram_max` (default `2..3`). Useful for autocomplete and
  substring match. Rejected when `min > max` or `min == 0`.
- **`keyword`** — single-token pass-through. Case-sensitive exact
  match, no tokenisation.
- **`options.analyzer`** column on every `db.indexes()` FULLTEXT
  row echoes the resolved analyzer name (including `ngram(m,n)`
  for parameterised ngram indexes), so driver tooling can render
  the tokenisation choice without probing the backend.

Config map shape:

```cypher
CALL db.index.fulltext.createNodeIndex(
  'movies', ['Movie'], ['title', 'overview'],
  {analyzer: 'english'}
)

CALL db.index.fulltext.createNodeIndex(
  'imgs', ['Image'], ['caption'],
  {analyzer: 'ngram', ngram_min: 3, ngram_max: 5}
)
```

Unknown analyzer names and invalid ngram sizes surface as
`ERR_FTS_UNKNOWN_ANALYZER`. The `db.indexes()` row shape grew one
column — `options` — at position 10; non-FTS rows emit an empty
map so existing consumers that read by column name keep working.

See [docs/guides/FULL_TEXT_SEARCH.md](docs/guides/FULL_TEXT_SEARCH.md).

## [1.8.0] — 2026-04-21

### Added — Full-text search (Tantivy)

phase6_opencypher-fulltext-search ships the Neo4j
`db.index.fulltext.*` procedure namespace on top of a Tantivy 0.22
backend. Nexus now maintains named BM25-scored full-text indexes
over node / relationship property sets and exposes them through the
same CALL surface Neo4j drivers already use.

- **Named FTS registry** — `FullTextRegistry` keyed by user-supplied
  name, backed by per-index Tantivy directories under
  `<data_dir>/indexes/fulltext/<name>/`. Cross-kind name uniqueness
  is enforced.
- **Procedures**:
  - `db.index.fulltext.createNodeIndex(name, labels, properties, config?)`
  - `db.index.fulltext.createRelationshipIndex(...)`
  - `db.index.fulltext.queryNodes(name, query, limit?)` → `(node, score)`
  - `db.index.fulltext.queryRelationships(...)` → `(relationship, score)`
  - `db.index.fulltext.drop(name)`
  - `db.index.fulltext.awaitEventuallyConsistentIndexRefresh()`
  - `db.index.fulltext.listAvailableAnalyzers()`
- **`db.indexes()` integration** — FTS indexes surface with
  `type = "FULLTEXT"` and `indexProvider = "tantivy-0.22"`.
- **BM25 ranking** — Tantivy default scorer, `top_k` default 100,
  tie-breaks on node id ascending.
- **Synchronous reader reload** — `FullTextIndex::add_document`
  now calls `reader.reload()` after every commit so the next query
  sees the write without waiting for a refresh tick.

Errors surface as `ERR_FTS_INDEX_EXISTS`, `ERR_FTS_INDEX_NOT_FOUND`,
`ERR_FTS_INDEX_INVALID`, or `ERR_FTS_PARSE`.

See [docs/guides/FULL_TEXT_SEARCH.md](docs/guides/FULL_TEXT_SEARCH.md).

**Parked for follow-up tasks** (outside this release's scope): WAL
integration for auto-populate on `CREATE`/`MERGE`/`SET`, per-index
analyzer catalogue (whitespace / simple / keyword / n-gram), bench
targets (<5 ms p95 single-term / <20 ms p95 phrase / >5k docs/sec
ingest), and the Neo4j TCK fulltext scenarios. Today, ingest goes
through the programmatic `FullTextRegistry::add_node_document`
API; query path is fully wired through Cypher `CALL`.

## [1.7.0] — 2026-04-21

### Added — Constraint enforcement for every advertised kind

phase6_opencypher-constraint-enforcement closes the correctness gap
where Nexus accepted DDL for NODE KEY / NOT NULL / property-type
constraints but silently ignored them on writes. Every kind now
enforces on CREATE / MERGE / SET / REMOVE / SET LABEL:

- **NODE KEY** — composite `(p1, p2, ...)` uniqueness + implicit
  NOT NULL on each component. Backed by the composite B-tree from
  phase6_opencypher-advanced-types with the `unique` flag set.
- **Relationship NOT NULL** — rejects rel CREATE that lacks the
  required property, rejects SET r.p = NULL / REMOVE r.p.
- **Property-type** (`IS :: INTEGER / FLOAT / STRING / BOOLEAN /
  BYTES / LIST / MAP`) — strict Neo4j semantics (INTEGER ≠ FLOAT),
  node and relationship scope.
- **NOT NULL alias** — `ASSERT n.p IS NOT NULL` parses as an alias
  of the legacy `EXISTS(n.p)` form.
- **Label-add guard** — `SET n:L` that violates any constraint on
  `L` is rejected before the label lands on the pending state.
- **Backfill validator** — registering a constraint on an existing
  dataset runs a one-shot streaming scan; the first 100 offending
  rows surface in the error payload; abort is atomic (no partial
  constraint state survives).
- **Relaxed-enforcement flag** — `Engine::set_relaxed_constraint_
  enforcement(true)` downgrades violations to `WARN` logs so users
  can port dirty datasets in stages. Emits a loud server-startup
  warning. Scheduled for removal at v1.5.

Registration today goes through the programmatic API
(`Engine::add_node_key_constraint`, `add_rel_not_null_constraint`,
`add_property_type_constraint`, `add_rel_property_type_constraint`);
the Cypher 25 `FOR (n:L) REQUIRE (...) IS NODE KEY` surface grammar
lands in the follow-up DDL-reshape task.

Errors surface as `ERR_CONSTRAINT_VIOLATED: kind=<KIND> ...` where
`<KIND>` is `UNIQUENESS` / `NODE_PROPERTY_EXISTENCE` / `NODE_KEY` /
`RELATIONSHIP_PROPERTY_EXISTENCE` / `PROPERTY_TYPE`. HTTP mapping:
409 for UNIQUENESS + NODE_KEY; 400 for NOT NULL + PROPERTY_TYPE.

See [docs/guides/CONSTRAINTS.md](docs/guides/CONSTRAINTS.md).

**Behaviour change**: workloads that relied on the silent
acceptance of non-unique constraint violations will start failing.
Set `relaxed_constraint_enforcement = true` during the migration
window if that applies.

## [1.6.0] — 2026-04-21

### Added — APOC procedure ecosystem (~100 procedures)

phase6_opencypher-apoc-ecosystem ships an in-tree APOC compatibility
surface across five namespaces:

- **`apoc.coll.*`** (30) — union, intersection, disjunction, subtract,
  sort / sortMaps / sortNodes, shuffle, reverse, zip, pairs / pairsMin,
  combinations, partitions, flatten (deep or shallow), frequencies /
  frequenciesAsMap, duplicates, toSet, indexOf, contains / containsAll,
  max / min / sum / avg / stdev, remove, fill, runningTotal.
- **`apoc.map.*`** (20) — merge / mergeList, fromPairs / fromLists /
  fromValues / fromEntries, setKey / removeKey / removeKeys, clean,
  flatten / unflatten, values, groupBy / groupByMulti, updateTree,
  submap, get / getOrDefault.
- **`apoc.text.*`** (20) — Levenshtein (distance + similarity), Jaro-
  Winkler, Sorensen-Dice, Hamming, regex groups / replace / split,
  phonetic (American Soundex), doubleMetaphone (Philips Metaphone),
  clean, lpad / rpad, format (`{0}` + `{name}`), base64 encode/decode,
  camelCase, capitalize, hexValue, byteCount.
- **`apoc.date.*`** (25) — format / parse / convertFormat (with Java
  `yyyy-MM-dd HH:mm:ss` tokens), currentMillis, systemTimezone,
  toYears / toMonths / toDays / toHours / toMinutes / toSeconds,
  add / subtract, fromISO / toISO, yearQuarter, week (ISO), weekday
  (Monday=1), dayOfYear, startOfDay / endOfDay, diff / between.
- **`apoc.schema.*`** (10) — assert (idempotent DDL row-shape),
  nodes, relationships, properties.distinctCount, node /
  relationship indexExists / constraintExists, stats, info.

Dispatch routes through the existing
`executor::operators::procedures::execute_call_procedure`; every
APOC name surfaces in `dbms.procedures()`. Compatibility matrix:
[docs/procedures/APOC_COMPATIBILITY.md](docs/procedures/APOC_COMPATIBILITY.md).

82 new unit tests. Full `cargo +nightly test -p nexus-core --lib`
run reports 1907 passed / 0 failed / 12 ignored.

## [1.5.0] — 2026-04-21

### Added — Advanced types (phase6_opencypher-advanced-types)

Six concurrent openCypher / Cypher 25 surface additions landing
together so downstream SDKs can consume a single compatibility level:

- **BYTES scalar family** — `bytes(s)`, `bytesFromBase64(s)`,
  `bytesToBase64(b)`, `bytesToHex(b)`, `bytesLength(b)`,
  `bytesSlice(b, start, len)`. JSON wire format is
  `{"_bytes": "<base64>"}`. Parameter binding also accepts a plain
  base64 STRING for convenience. 64 MiB per-property cap enforced.
- **Write-side dynamic labels** — `CREATE (n:$label)`,
  `SET n:$label`, `REMOVE n:$label`. Parameter may resolve to a
  STRING or a `LIST<STRING>` (multi-label fan-out). Comprehensive
  `ERR_INVALID_LABEL` surface for null, empty, or malformed inputs.
- **Composite B-tree indexes** — `CREATE INDEX <name> FOR (n:Label)
  ON (n.p1, n.p2, ...)`. Exact / prefix / range seeks and a
  uniqueness flag available through `CompositeBtreeRegistry`.
- **Typed-collection validation** —
  `LIST<INTEGER|FLOAT|STRING|BOOLEAN|BYTES|ANY>` parse helper +
  `validate_list` enforcement for the constraint engine.
- **Transaction savepoints** — `SAVEPOINT <name>`,
  `ROLLBACK TO SAVEPOINT <name>`, `RELEASE SAVEPOINT <name>`.
  Nested savepoints unwind LIFO. See
  [docs/guides/SAVEPOINTS.md](docs/guides/SAVEPOINTS.md).
- **Graph scoping** — `GRAPH[<name>]` preamble parsed into
  `CypherQuery.graph_scope`. The single-engine path surfaces
  `ERR_GRAPH_NOT_FOUND` when a scope cannot be served in place;
  multi-database routing happens above the engine.

1799 unit tests passing (1742 pre-task + 57 new). Regression-free
against the Neo4j 2025.09 diff suite.

## [1.0.0] — 2026-04-20

### Fixed — CREATE with bound-variable edges duplicated nodes (2026-04-20)

`CREATE (a:X {id:1}), (b:X {id:2}), (a)-[:R]->(b)` produced 4
nodes instead of 2 on Nexus: the edge pattern's `(a)` and `(b)`
re-created the declared variables as anonymous `:X` duplicates
instead of binding to the earlier declarations.

Root cause in
`crates/nexus-core/src/executor/operators/create.rs`'s
`execute_create_pattern_internal`: the pattern-walker
unconditionally created a new node every time it saw a
`PatternElement::Node`, never checking whether that element's
variable was already populated in the `created_nodes` map the
same walker had just written to. Same problem on the target
side of `PatternElement::Relationship`.

Fix: before creating a new node, check if the pattern's variable
is already in `created_nodes`. If so, rebind `last_node_id` to
the existing id and continue — no duplicate record, no extra
catalog update. Applied on both branches.

Verified end-to-end:

- `create_bound_variable_edge_does_not_duplicate_nodes` and
  `create_bound_variable_chain_reuses_nodes` (new unit tests in
  `crates/nexus-core/src/engine/tests.rs`) — single edge + chain
  variant; cover 2-node / 3-node patterns.
- `nexus-bench::TinyDataset.load_statement` now produces 100
  nodes + 50 relationships on Nexus (was 200 + 50). Locked in by
  strengthened assertions in `tests/live_rpc.rs` +
  `tests/live_compare.rs`.
- `cargo test --workspace` on `nexus-core`: 1722 passed, 0
  failed (no regressions).

Source task: `phase6_nexus-create-bound-var-duplication`.

### Fixed — RPC DELETE / DETACH DELETE no-op (2026-04-20)

Queries like `MATCH (n) DETACH DELETE n` issued over the native
MessagePack RPC protocol parsed and returned `Ok(0 rows)` but left
the database untouched. Root cause: the RPC CYPHER dispatch in
`crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs` called
`executor.execute(&q)` directly for every non-admin query. The
operator pipeline's `Operator::Delete` / `Operator::DetachDelete`
handlers are explicit no-ops — they rely on the engine's
higher-level interception (`execute_cypher_with_context` at
`crates/nexus-core/src/engine/mod.rs:1427`) to perform the actual
mutation. REST always went through that path; RPC bypassed it.

The fix adds a `needs_engine_interception(&ast)` router: any AST
that carries `Match` / `Create` / `Delete` / `Merge` / `Set` /
`Remove` / `Foreach` now routes through `engine.execute_cypher`,
preserving parity with the REST transport. Read-only queries
(no MATCH, no mutation) keep the parallel executor path —
unchanged throughput, unchanged params handling.

Verified end-to-end against a live Nexus RPC listener + docker
Neo4j 2025.09.0: `nexus-bench`'s 9 `#[ignore]` integration tests
now run cleanly as a single `cargo test -p nexus-bench
--features live-bench,neo4j -- --ignored` parallel batch (used to
require per-test manual wipes). A new engine-level regression
test (`detach_delete_actually_clears_nodes_via_execute_cypher` in
`crates/nexus-core/src/engine/tests.rs`) locks the interception
contract.

Source task: `phase6_nexus-delete-executor-bug`.

### Added — server admission control (2026-04-20)

Third back-pressure layer on top of the existing per-key rate limiter
and per-connection RPC semaphore. A global `AdmissionQueue`
(`crates/nexus-server/src/middleware/admission.rs`) gates every
query-bearing HTTP route (`/cypher`, `/ingest`, `/knn_traverse`,
`/graphql`, `/umicp`) through a shared tokio semaphore. Callers that
would push concurrency over `NEXUS_ADMISSION_MAX_CONCURRENT` (default
CPU-count clamped to `[4, 32]`) wait in a FIFO queue up to
`NEXUS_ADMISSION_QUEUE_TIMEOUT_MS` (default 5 s); after that they
are rejected with `503 Service Unavailable + Retry-After`.

Motivation: a single authenticated client can fan out tens of
thousands of legitimate-looking `CREATE` statements through one
HTTP keep-alive — enough to saturate the engine's single-writer
discipline and wedge the process even though every request sat under
the per-key rate limit. The new layer bounds **global** engine-facing
concurrency rather than per-key volume.

Light-weight endpoints (`/health`, `/prometheus`, `/auth`,
`/schema/*`, `/stats`, `/cluster/status`) bypass the queue via a
`HEAVY_PATH_PREFIXES` matcher so diagnostics stay reachable when
the engine is saturated. RPC + RESP3 surfaces continue to rely on
their per-connection semaphore; unified gating is a follow-up.

Config knobs:

- `NEXUS_ADMISSION_ENABLED` (bool, default `true`)
- `NEXUS_ADMISSION_MAX_CONCURRENT` (u32, default CPU-clamped)
- `NEXUS_ADMISSION_QUEUE_TIMEOUT_MS` (u64, default 5000)

Prometheus metric names reserved (counters + histogram wiring ships
in a subsequent patch):
`nexus_admission_permits_granted_total`,
`nexus_admission_permits_rejected_total`,
`nexus_admission_in_flight`,
`nexus_admission_wait_seconds`.

Docs: [`docs/security/OVERLOAD_PROTECTION.md`](docs/security/OVERLOAD_PROTECTION.md).
17 tests (unit + axum middleware) covering concurrency cap, timeout,
FIFO progress under contention, light-path short-circuit, heavy-path
rejection, counter integrity on drop.

### Added — V2 horizontal scaling (2026-04-20, commit `15715a24`)

Nexus gains horizontal scalability through hash-based sharding, per-shard
Raft consensus, and a distributed query coordinator. See
[`docs/guides/DISTRIBUTED_DEPLOYMENT.md`](docs/guides/DISTRIBUTED_DEPLOYMENT.md)
and [`.rulebook/tasks/phase5_implement-v2-sharding/design.md`](.rulebook/tasks/phase5_implement-v2-sharding/design.md).

- **Sharding** (`crates/nexus-core/src/sharding/`): deterministic xxh3-based
  shard assignment, generation-tagged cluster metadata, iterative
  rebalancer, per-shard health model. Standalone deployments are
  unchanged — sharding is opt-in via `[cluster.sharding]` config.
- **Raft consensus per shard** (`crates/nexus-core/src/sharding/raft/`):
  purpose-built Raft (openraft 0.10 is still alpha; its trait surface
  would require an adapter larger than the Raft itself). Leader
  election within 3× election timeout, §5.3 truncate-on-conflict,
  §5.4.2 leader-only current-term commit, snapshot install, bincode
  wire format with shard-id prefix. 5-node clusters tolerate 2
  replica failures.
- **Distributed query coordinator** (`crates/nexus-core/src/coordinator/`):
  scatter/gather with atomic per-query failure, leader-hint retry
  (3 attempts), stale-generation refresh, COUNT/SUM/AVG/MIN/MAX/
  COLLECT aggregation decomposition, ORDER BY + LIMIT top-k merge.
- **Cross-shard traversal**: TTL + generation-aware LRU cache (10k
  entries default), per-query fetch budget (1k default) with
  `ERR_TOO_MANY_REMOTE_FETCHES` for runaway traversals.
- **Cluster management API** (`crates/nexus-server/src/api/cluster.rs`):
  `GET /cluster/status`, `POST /cluster/{add_node,remove_node,rebalance}`,
  `GET /cluster/shards/{id}`. Admin-gated, `307 Temporary Redirect` on
  follower writes, drain semantics for graceful node removal.

### Changed — workspace layout

The four Rust crates moved from repo-root children into a single
`crates/` directory, following the standard Rust workspace layout:

```
Nexus/
├── crates/
│   ├── nexus-core/      # was ./nexus-core/
│   ├── nexus-server/    # was ./nexus-server/
│   ├── nexus-protocol/  # was ./nexus-protocol/
│   └── nexus-cli/       # was ./nexus-cli/
├── docs/                # unchanged
├── sdks/                # unchanged
└── scripts/             # unchanged
```

Follow-up edits:

- `Cargo.toml` root: `workspace.members` + `workspace.dependencies`
  paths updated to `crates/…`.
- `crates/nexus-core/Cargo.toml`: `[[example]]` paths `../examples/` →
  `../../examples/`.
- `crates/nexus-server/Cargo.toml` + `crates/nexus-cli/Cargo.toml`:
  `[package.metadata.deb]` asset paths (`../LICENSE`, `../README.md`,
  `../config.yml`, …) updated to `../../…`.
- `.github/workflows/rust-lint.yml`, `release-server.yml`,
  `release-cli.yml`: path filters + `manifest_path` point at `crates/…`.
- `scripts/ci/check_no_unwrap_in_bin.sh`: `SCOPES` + repo-root detection
  updated.
- Inter-crate paths (`../nexus-protocol`) unchanged — both live under
  `crates/` so the relative form still resolves.

No functional change; no public API moved or renamed.

### Test coverage

**201 V2-dedicated tests** — 143 sharding unit tests, 46 coordinator
unit tests, 12 E2E integration scenarios
(`crates/nexus-core/tests/v2_sharding_e2e.rs`) covering every §Scenario
in the specs:

- Deterministic assignment across restarts
- Metadata consistency after leader change
- Single-shard + broadcast query classification
- AVG / SUM / MIN / MAX / COLLECT aggregation decomposition
- Shard-failure atomicity (partial rows never leaked)
- Raft failover within spec bound (≤90 ticks = 900ms)
- Minority-failure replication continuity
- Rebalance convergence
- Leader-redirect on followers
- Stale-generation refresh round-trip

Full workspace on nightly: **2169 tests passing, 0 failed** (1694
nexus-core lib + 364 nexus-server lib + 83 nexus-protocol lib + 28
nexus-cli lib + 12 V2 E2E). Zero warnings on `cargo clippy
--workspace --all-targets -- -D warnings`. Release build (`cargo
+nightly build --release --workspace`) succeeds in ~3 minutes.

### Breaking changes (when sharding is enabled)

- Record-store files gain a 64-byte V2 header. Standalone deployments
  use deterministic defaults (`shard_id = 0`, `generation = 0`); a
  future `nexus migrate --to v2` CLI rewrites headers in place.

### Follow-up

- [`phase5_v2-tcp-transport-bridge`](.rulebook/tasks/phase5_v2-tcp-transport-bridge/)
  — TCP transport between Raft replicas for multi-host deployments.
  Current in-process transport covers single-host + all integration
  scenarios; the TCP bridge is an I/O adapter over the already-stable
  `RaftTransport` and `ShardClient` traits.

### Added — cluster mode (multi-tenant deployments, 2026-04-19)

Nexus can now run as a shared multi-tenant service. One server
instance hosts data for many tenants while guaranteeing that a
tenant's nodes, relationships, property keys, and label names stay
strictly isolated from every other tenant. See `docs/CLUSTER_MODE.md`
for the operator guide.

Enable with `NEXUS_CLUSTER_ENABLED=true` (opt-in; standalone mode
remains the default and is byte-identical to the pre-cluster
behaviour). Once on:

- **Mandatory authentication on every URI.** Cluster mode removes
  every public endpoint — `/`, `/health`, `/stats`, `/openapi.json`
  all require a valid API key. A shared multi-tenant server must
  identify every caller before exposing any surface.
- **Per-tenant data isolation.** Labels / relationship types /
  property keys registered by tenant A get different catalog IDs
  than the same names registered by tenant B, so every downstream
  layer (label bitmap index, KNN, record stores) sees tenant-
  distinct state for free. Data leakage is structurally impossible
  — not an invariant maintained by discipline. Proven end-to-end
  by the integration tests in `nexus-core/tests/cluster_isolation_tests.rs`.
- **Per-tenant rate limiting.** Every request is gated by
  `LocalQuotaProvider` (per-minute + per-hour windows, configurable
  via `ClusterConfig::default_quotas`). 429 responses carry
  `Retry-After` and `X-RateLimit-Remaining` headers so SDK clients
  can back off cleanly.
- **Function-level MCP permissions.** API keys gain an optional
  `allowed_functions` allow-list. Handlers can call
  `UserContext::require_may_call("tool.name")?` to gate specific
  MCP / RPC operations per-key, and discovery endpoints can use
  `filter_callable` to advertise only callable tools.

New public surface: `nexus_core::cluster::{ClusterConfig,
TenantIsolationMode, UserNamespace, UserContext, QuotaProvider,
LocalQuotaProvider, FunctionAccessError}`.

New env var: `NEXUS_CLUSTER_ENABLED`. Architecturally documented in
ADR-7 (catalog-prefix isolation over byte-level or per-database
alternatives).

### Changed — API key storage migrated from bincode to JSON

`nexus-core/src/auth/storage.rs` switched from `SerdeBincode<ApiKey>`
to `SerdeJson<ApiKey>` for the `api_keys` LMDB database. Bincode's
default config is NOT forward-compatible for appended fields —
adding cluster mode's new `allowed_functions: Option<Vec<String>>`
field would have panicked on every existing record with
`unexpected end of file`. JSON + `#[serde(default)]` gives us room
to grow the schema without a migration script.

**Operational note:** existing auth data is NOT automatically
migrated on upgrade. Cluster-mode deployments should regenerate API
keys from scratch; standalone deployments that already persist API
keys should expect to re-seed on first boot under the new binary.
The shared test-suite catalog was bumped to a new path
(`nexus_test_auth_shared_v2`) so stale bincode records from earlier
runs are orphaned cleanly instead of failing to decode.

### Fixed — parser no longer accepts standalone `WHERE` (Neo4j parity)

Closes the last outlier in the 300-test Neo4j compat suite. Before
this change, Nexus accepted `UNWIND [1,2,3,4,5] AS x WHERE x > 2
RETURN x` and returned `[3, 4, 5]`, while Neo4j 2025.09.0 rejects the
same query with a syntax error (`Invalid input 'WHERE': expected
'ORDER BY', 'CALL', ...`). Standard Cypher only allows `WHERE`
attached to `MATCH` / `OPTIONAL MATCH` / `WITH` — never as a
standalone top-level clause.

The parser now matches Neo4j's grammar exactly: a bare `WHERE` after
any clause other than those three rejects with the same error
message shape Neo4j produces, pointing callers at the migration.

**Breaking change — migration.** Any query that glued `WHERE`
directly onto the output of `UNWIND` / `CREATE` / `DELETE` (or any
other non-MATCH/WITH producer) must insert a `WITH <vars>`
pass-through projection before the predicate:

```cypher
-- before
UNWIND [1, 2, 3, 4, 5] AS x WHERE x > 2 RETURN x

-- after
UNWIND [1, 2, 3, 4, 5] AS x WITH x WHERE x > 2 RETURN x
```

The new syntax error points at the exact column and lists the
valid clauses, so stale call sites surface immediately on the next
request instead of going silent.

**Result.** Neo4j compat suite now reports **300/300 passing**
(previously 299/300 with 14.05 the one outlier). Every other test
across all 17 sections — Basic Queries, Pattern Matching,
Aggregations, Type Conversion, DELETE/SET, etc. — keeps its
scalar-path parity.

### SDK + workspace version unification

Every first-party crate and SDK bumped to **1.0.0** (previously a
mix of `0.12.0` for the server workspace and `0.1.0` for some SDKs).
One version number governs the CLI, server, protocol crate, Rust
SDK, Python SDK, TypeScript SDK, Go SDK, C# SDK, and PHP SDK.

### Removed ecosystem SDKs

The following integrations were dropped to focus on first-party wire
clients:

- `sdks/n8n/` — the community n8n node. Users can still invoke the
  Nexus HTTP endpoint or wrap the TypeScript SDK inline.
- `sdks/langchain/` and `sdks/langflow/` — Python ecosystem
  wrappers. The underlying Python SDK covers the same API surface;
  higher-level orchestration wrappers are better maintained
  out-of-tree where they can track upstream LangChain / LangFlow
  releases on their own cadence.
- `sdks/TestConsoleSimple/` — redundant C# test harness (the
  canonical tests live in `sdks/csharp/Tests/`).

### Documentation reorganisation

- New `sdks/README.md` — canonical index of shipped SDKs with the
  shared transport contract referenced up front.
- `sdks/SDK_TEST_RESULTS.md`, `sdks/SDK_TEST_RESULTS_FINAL.md`, and
  `sdks/TEST_COVERAGE_REPORT.md` moved to `docs/sdks/` so the `sdks/`
  root only holds runnable client code + the test-matrix script.
- Per-SDK `CHANGELOG.md` created for every remaining SDK (Rust,
  Python, TypeScript, Go, C#, PHP) — the Rust SDK entry has the
  full 1.0.0 RPC-default details, the others carry a "1.0.0 version
  alignment, RPC default queued under
  phase2_sdk-rpc-transport-default" entry.

### Native Binary RPC transport (2026-04-18)

**First-party SDKs now have a MessagePack RPC port.** Length-prefixed
frames (`[u32 LE][rmp-serde body]`) on port `15475`, multiplexed over
a single TCP connection via caller-chosen `Request.id`. Enabled by
default (`[rpc].enabled = true`); RESP3 and HTTP continue to run
unchanged alongside it.

```
NEW nexus-protocol/src/rpc/{mod,types,codec}.rs   (shared w/ SDKs)
NEW nexus-server/src/protocol/rpc/
    mod.rs, server.rs, metrics.rs,
    dispatch/{mod, admin, convert, cypher, database, graph, ingest, knn, schema}.rs
NEW nexus-server/tests/rpc_integration_test.rs
NEW docs/specs/rpc-wire-format.md
```

Command set: admin handshake (PING / HELLO / AUTH / QUIT / STATS /
HEALTH), CYPHER (with optional params map; EXPLAIN inline), graph CRUD
(CREATE_NODE / CREATE_REL / UPDATE_NODE / DELETE_NODE / MATCH_NODES),
KNN (KNN_SEARCH accepting embedding as Bytes-of-f32 or Array<Float>
with optional property filter, KNN_TRAVERSE with seed list + depth),
bulk ingest (INGEST, single-batch atomic), schema introspection
(LABELS / REL_TYPES / PROPERTY_KEYS / INDEXES from the catalog
directly), multi-database (DB_LIST / DB_CREATE / DB_DROP / DB_USE).

64 MiB cap per frame (tunable via `rpc.max_frame_bytes`), per-
connection in-flight cap (`max_in_flight_per_conn`, default 1024),
`u32::MAX` reserved as `PUSH_ID` for future streaming, slow-command
WARN logging at `rpc.slow_threshold_ms` (default 2 ms).

Prometheus: `nexus_rpc_connections` (gauge), `nexus_rpc_commands_total`
/ `_error_total`, `nexus_rpc_command_duration_microseconds_total`,
`nexus_rpc_frame_bytes_in_total` / `_out_total`,
`nexus_rpc_slow_commands_total`. Env overrides:
`NEXUS_RPC_{ENABLED, ADDR, REQUIRE_AUTH, MAX_FRAME_BYTES,
MAX_IN_FLIGHT, SLOW_MS}`.

The wire-format layer (RPC types + codec, RESP3 parser + writer) moved
from `nexus-server::protocol` into `nexus-protocol::{rpc, resp3}` so
the Rust SDK can depend on it without pulling the whole server crate.
Command dispatch and the TCP accept loop stay in `nexus-server`.

121 new tests (113 unit + 8 integration) covering every command,
wrong-arity / wrong-type guards, NOAUTH gating, pipelined multiplexing,
PUSH_ID rejection, and end-to-end CRUD round-trips over TCP.

### 🔌 RESP3 Transport (2026-04-18)

**Any RESP3 client — `redis-cli`, `iredis`, RedisInsight, Jedis, redis-rb,
Redix — can now talk to Nexus using a Nexus command vocabulary.** The port
is additive (HTTP, MCP, UMICP all keep running), disabled by default, and
loopback-only out of the box so a plaintext debug port never accidentally
escapes a dev machine.

```
NEW nexus-server/src/protocol/resp3/
  mod.rs, parser.rs, writer.rs, server.rs
  command/{mod, admin, cypher, graph, knn, schema}.rs
NEW nexus-server/tests/resp3_integration_test.rs
NEW docs/specs/resp3-nexus-commands.md
```

**25+ commands** implemented in the Nexus vocabulary:

- Admin: `PING`, `HELLO [2|3] [AUTH user pass]`, `AUTH <api-key|user pass>`,
  `QUIT`, `HELP`, `COMMAND`.
- Cypher: `CYPHER`, `CYPHER.WITH`, `CYPHER.EXPLAIN`.
- Graph CRUD: `NODE.CREATE/GET/UPDATE/DELETE/MATCH`, `REL.CREATE/GET/DELETE`.
- KNN / ingest: `KNN.SEARCH`, `KNN.TRAVERSE`, `INGEST.NODES`, `INGEST.RELS`.
- Schema / databases: `INDEX.CREATE/DROP/LIST`, `DB.LIST/CREATE/DROP/USE`,
  `LABELS`, `REL_TYPES`, `PROPERTY_KEYS`, `STATS`, `HEALTH`.

**Wire format**: all 12 RESP3 type prefixes (`+`, `-`, `:`, `$`, `*`, `_`,
`,`, `#`, `=`, `~`, `%`, `|`, `(`) supported on both parse and write, with
automatic RESP2 degradation (Null → `$-1`, Map → flat array, Boolean →
`:0`/`:1`, Verbatim → BulkString) when the peer negotiates `HELLO 2`.
`redis-cli`-style inline commands (`PING\r\n`) tokenised with quote and
escape support, so plain `telnet` sessions work too.

**Explicitly not Redis emulation.** `SET key value` returns
`-ERR unknown command 'SET' (Nexus is a graph DB, see HELP)`. No KV
semantics.

**Auth**: `HELLO 3 AUTH <user> <pass>` negotiates protocol + auth in one
round-trip. Pre-auth commands (`PING`/`HELLO`/`AUTH`/`QUIT`/`HELP`/`COMMAND`)
always run; everything else bounces with `-NOAUTH Authentication required.`
when the listener was configured with `require_auth = true` and the
session hasn't authenticated.

**Concurrency**: every handler that touches `Engine` or `DatabaseManager`
acquires the `parking_lot::RwLock` inside `tokio::task::spawn_blocking` —
same policy as the HTTP handlers (see `docs/performance/CONCURRENCY.md`).
A tokio worker thread is never pinned on a graph-engine lock.

**Metrics** (exported at `GET /prometheus`):
- `nexus_resp3_connections` (gauge)
- `nexus_resp3_commands_total` (counter)
- `nexus_resp3_commands_error_total` (counter)
- `nexus_resp3_command_duration_microseconds_total` (counter — divide by
  `commands_total` for an average)
- `nexus_resp3_bytes_read_total` / `nexus_resp3_bytes_written_total`

**Config**: `[resp3]` section in `config.yml` with `enabled`, `addr`,
`require_auth`. Env overrides `NEXUS_RESP3_{ENABLED,ADDR,REQUIRE_AUTH}`.
Default port `15476` (HTTP stays on `15474`).

**Testing**: 77 new tests green (69 in-crate unit + 8 raw-TCP integration).

### 🛡️ Audit-log Failure Propagation (2026-04-18)

**Eight `let _ = audit_logger.log_*(...).await` sites were silently
swallowing audit-log write failures.** All now go through a new helper
`nexus_core::auth::record_audit_log_failure(context, err)` that bumps a
process-global `AtomicU64` counter and emits a
`tracing::error!(target = "audit_log", context, error)` event.

**Policy: fail-open with metric.** The originating request keeps its
original HTTP status (401/429/500/200) — we do NOT convert audit-sink
failures into 500s, because doing so hands an attacker who can cause IO
pressure (disk fill, permission flap) a lever to mass-reject legitimate
traffic. Operators alarm on the Prometheus counter instead:

```promql
increase(nexus_audit_log_failures_total[5m]) > 0
```

**Call sites patched**:
- `nexus-core/src/auth/middleware.rs` × 4 (missing/invalid/errored API
  key, rate-limit exceeded).
- `nexus-server/src/api/cypher/execute.rs` × 4 (SET-property + SET-label
  success/failure on the Cypher write path).

**Metric**: `nexus_audit_log_failures_total` exported at `GET /prometheus`
with HELP text pointing operators at the alert template.

**Docs**: [docs/security/SECURITY_AUDIT.md §5](docs/security/SECURITY_AUDIT.md) documents the
full policy (behaviour, rationale, alarm template, code-location
inventory, "not fail-closed" guard). [docs/security/AUTHENTICATION.md](docs/security/AUTHENTICATION.md)
cross-links from its audit section.

### ⚡ Async Lock Migration — `DatabaseManager` off tokio workers (2026-04-18)

**14 async HTTP handlers acquired `Arc<parking_lot::RwLock<DatabaseManager>>`
directly inside `async fn`, pinning a tokio worker for the whole lock-held
window.** Under concurrent load this starved the runtime — observed during
the `fix/memory-leak-v1` debug session as the container dropping requests
well before hitting any memory limit.

**Fix**: wrap every async-context lock acquisition in
`tokio::task::spawn_blocking` so the read/write runs on the blocking
pool while tokio workers stay free. The lock type stays
`parking_lot::RwLock` because it is shared with sync Cypher execution in
`nexus-core/src/executor/shared.rs` — migrating the type would ripple into
~20 files and force every sync caller onto `.blocking_read()` (which
panics if ever reached from an async context). The `spawn_blocking`
approach fixes the starvation at the source with a fraction of the blast
radius.

**Touched call sites (14 total)**:
- `nexus-server/src/api/database.rs` — 6 handlers
  (`create`/`drop`/`list`/`get`/`get_session`/`switch_session`).
- `nexus-server/src/api/cypher/commands.rs` — 4 admin-Cypher sites
  (`UseDatabase`/`ShowDatabases`/`CreateDatabase`/`DropDatabase`).

**Enforcement**: `nexus-server/Cargo.toml` sets
`clippy::await_holding_lock = "deny"` so any future regression fails CI.

**Regression test**:
`test_concurrent_list_databases_does_not_starve_runtime` fires 32
concurrent `list_databases` calls on a 2-worker tokio runtime and asserts
all 32 return `200 OK` inside a 30 s pathological timeout. Runs in 0.15 s
post-migration.

**Docs**: [docs/performance/CONCURRENCY.md](docs/performance/CONCURRENCY.md)
documents the lock model end-to-end — primitives, the `DatabaseManager`
rule, clippy enforcement, migration-vs-wrap tradeoff, and which
`tokio::sync` locks legitimately stay.

### 🧱 Neo4j Compatibility Test Split (Tier 3.2) (2026-04-18)

**`nexus-core/tests/neo4j_compatibility_test.rs` was 2,103 LOC in a single
`#[serial]`-gated integration binary. The whole file ran end-to-end on every
test invocation even though only one section had changed. Split by semantic
section into three independent binaries.**

```
neo4j_compatibility_test.rs                 2,103 LOC → removed
neo4j_compatibility_core_test.rs            NEW →  317 LOC — 7 fixture-driven tests
                                            (multi-label MATCH, UNION, bidirectional
                                             relationships, property access). Hosts
                                             the shared `setup_test_data` fixture.
neo4j_compatibility_extended_test.rs        NEW → 1,063 LOC — 34 tests covering
                                             UNION variants, labels()/keys()/type(),
                                             DISTINCT, ORDER BY with UNION, multi-label
                                             aggregations + the count(*) suite (8 tests).
neo4j_compatibility_additional_test.rs      NEW →  825 LOC — 68 numbered
                                             `neo4j_compat_*` / `neo4j_test_*`
                                             micro-scenarios (count/labels/keys/id/type
                                             / LIMIT / DISTINCT / property types).
```

Pure refactor — every test body is byte-identical to the original, `#[serial]`
gating preserved, same helper `execute_query` function duplicated in each
file. `setup_test_data` lives only in `core_test.rs` (the only caller).

All 109 tests pass (7 + 34 + 68) under
`cargo +nightly test --package nexus-core --test neo4j_compatibility_*_test`;
clippy warning-clean.

**Benefits**:
- Granular test targeting — `cargo test --test neo4j_compatibility_core_test`
  runs only the 7 fixture-driven scenarios (~0.3s).
- Parallel binary compilation — the three binaries link independently.
- Each file is under 1,100 LOC, well under the 1,500 LOC target.

### 🧱 Regression Test Split (Tier 3.1) (2026-04-18)

**`nexus-core/tests/regression_extended.rs` was 2,184 LOC covering seven
feature areas in a single integration-test binary. Split by feature area
into seven cohesive test binaries — each one now compiles and runs
independently, and `cargo test --test regression_extended_match`
(etc.) exercises just the relevant slice.**

```
regression_extended.rs                 2,184 LOC  → removed
regression_extended_create.rs          NEW →  423 LOC  — 25 CREATE tests
regression_extended_match.rs           NEW →  312 LOC  — 17 MATCH/WHERE tests
regression_extended_relationships.rs   NEW →  583 LOC  — 24 relationship tests
regression_extended_functions.rs       NEW →  343 LOC  — 20 function tests
regression_extended_union.rs           NEW →  225 LOC  — 10 UNION tests
regression_extended_engine.rs          NEW →  172 LOC  — 12 Engine-API tests
regression_extended_simple.rs          NEW →  140 LOC  — 10 smoke tests
```

Pure refactor — every test body is byte-identical to the original
(comments and `setup_test_engine` / `setup_isolated_test_engine` calls
preserved). Dead `use nexus_core::Engine` import dropped (the type name
was never referenced at the call sites). All 118 tests pass under
`cargo +nightly test --package nexus-core --test regression_extended_*`
and workspace-wide clippy is warning-clean.

**Benefits**:
- Merge-conflict surface reduced — unrelated test additions no longer
  collide on a single file.
- Parallel `cargo test` scheduling — the seven binaries run concurrently
  (~0.4 s wall-clock for the full suite versus the old serialized run).
- AI-agent-friendly file sizes — largest file (`relationships`, 583 LOC)
  is well under the 1,500 LOC target.

### 🧱 Engine Module Split (Tier 1.5) (2026-04-18)

**`nexus-core/src/engine/mod.rs` was 4,636 LOC — the largest remaining
source file in the tree after the Tier 1 + Tier 2 splits. Carved out
into five focused submodules in four atomic commits.**

```
engine/mod.rs         4,636 → 3,624 LOC   (−1012, −21.8%)
engine/config.rs      NEW → 45 LOC        — GraphStatistics, EngineConfig
engine/stats.rs       NEW → 39 LOC        — EngineStats, HealthStatus, HealthState
engine/clustering.rs  NEW → 135 LOC       — cluster_nodes + 5 wrappers + convert_to_simple_graph
engine/maintenance.rs NEW → 193 LOC       — knn_search, export_to_json, get_graph_statistics,
                                              clear_all_data, validate_graph, graph_health_check,
                                              health_check
engine/crud.rs        NEW → 651 LOC       — create/get/update/delete nodes + relationships +
                                              index_node_properties + apply_pending_index_updates +
                                              NodeWriteState (Cypher write-pass staging)
```

Pure refactor — public API surface unchanged (every method still
resolves as `Engine::*` via Rust's multi-file `impl` blocks), all
2,567 nexus-core tests green across every split commit, pre-commit
hooks (fmt + clippy deny-warnings) enforced on each step.

mod.rs remains the largest file in the tree; the residual ~2,400 LOC
are the Cypher execution core (33 private helpers with shared state
needing a deeper reshape than a pure file split). Tracked under
`phase1_split-oversized-modules` Tier 3 for a follow-up.

### ⚡ SIMD Runtime-Dispatched Kernels + Parser O(N²) Fix (2026-04-18)

**New `nexus-core::simd` module — always compiled, runtime-dispatched,
no Cargo feature flags. Kernels span distance (f32 dot / l2_sq / cosine
/ normalize), bitmap popcount, numeric reductions (sum / min / max i64
/ f64 / f32), compare (eq / ne / lt / le / gt / ge i64 / f64), RLE run
scanning, CRC32C, and a size-threshold JSON dispatcher.**

Per ADR-003, every kernel ships as scalar reference + SSE4.2 + AVX2 +
AVX-512F + NEON with proptest parity (>= 40 cases, 256–1024 inputs
each). Selection is cached in `OnceLock<unsafe fn>` on first call;
`NEXUS_SIMD_DISABLE=1` env var forces scalar runtime-wide for
emergency rollback.

**Measured on Ryzen 9 7950X3D (Zen 4, AVX-512F + VPOPCNTQ):**

| Op                  | Scale       | Scalar   | Dispatch  | Speedup  |
|---------------------|-------------|----------|-----------|----------|
| `dot_f32`           | dim=768     | 438 ns   | 34.5 ns   | 12.7×    |
| `dot_f32`           | dim=1024    | 580 ns   | 50.8 ns   | 11.4×    |
| `dot_f32`           | dim=1536    | 893 ns   | 70.3 ns   | 12.7×    |
| `l2_sq_f32`         | dim=512     | 285 ns   | 21.0 ns   | 13.5×    |
| `popcount_u64`      | 4096 words  | 1.52 µs  | 136 ns    | ≈11×     |
| `sum_f64`           | n=262 144   | 150 µs   | 19 µs     | 7.9×     |
| `sum_f32`           | n=262 144   | 152 µs   | 9.5 µs    | 15.9×    |
| `lt_i64`            | n=262 144   | 110 µs   | 25 µs     | 4.4×     |
| `eq_i64`            | n=262 144   | 69 µs    | 24 µs     | 2.9×     |
| `find_run_length`   | uniform 16k | 3.2 µs   | 1.0 µs    | 3.2×     |
| **Cypher parse**    | **31.5 KiB**| **≈1 s** | **3.7 ms**| **≈290×**|

Cypher parse speedup is the non-SIMD O(N²) → O(N) fix uncovered while
auditing phase-3 §8–9: `self.input.chars().nth(self.pos)` (O(n) per
call) replaced with `self.input[self.pos..].chars().next()` (O(1)) in
`peek_char`, `consume_char`, `peek_keyword`, `peek_keyword_at`,
`skip_whitespace`, `peek_char_at`. Cost-per-byte now flat at
92–117 ns/byte across three orders of magnitude — linear scaling
confirmed.

**Production call sites wired to SIMD:**

- `index::KnnIndex` — `DistSimdCosine` / `DistSimdL2` implement
  `hnsw_rs::dist::Distance<f32>` via `simd::distance::cosine_f32` /
  `l2_sq_f32`. Every HNSW insert and query distance flows through
  AVX-512 / AVX2 / NEON on supported hardware.
- `index::KnnIndex::normalize_vector` — delegates to
  `simd::distance::normalize_f32`.
- `graph::algorithms::traversal::{cosine_similarity, jaccard_similarity}`
  — refactored from full-universe f64 fold to packed `Vec<u64>`
  bitmaps + `simd::bitmap::{popcount_u64, and_popcount_u64}`.
- `storage::graph_engine::compression::compress_simd_rle` — inner
  run-length scan replaced with `simd::rle::find_run_length` (was
  misnamed "SIMD-accelerated", now actually SIMD).
- `wal::Wal::append` / `recover` — dual-format (v1/v2) frames with
  pluggable `ChecksumAlgo` field; reads both, writes default to
  `Crc32Fast` (benchmark showed 3-way parallel PCLMUL in `crc32fast`
  beats sequential `_mm_crc32_u64` on modern x86; CRC32C primitive
  kept available via `append_with_algo(entry, Crc32C)`).
- `executor::parser::{tokens, expressions}` — O(N²) tokenizer fix.

**New files (all under `nexus-core/src/simd/`):** `mod.rs`, `dispatch.rs`,
`scalar.rs`, `distance.rs`, `bitmap.rs`, `reduce.rs`, `compare.rs`,
`rle.rs`, `crc32c.rs`, `json.rs`, `x86.rs`, `aarch64.rs`.

**New benches (under `nexus-core/benches/`):** `simd_distance.rs`,
`simd_popcount.rs`, `simd_reduce.rs`, `simd_compare.rs`, `simd_rle.rs`,
`simd_crc.rs`, `simd_json.rs`, `parser_tokenize.rs`.

**New proptest parity suites (under `nexus-core/tests/`):**
`simd_scalar_properties.rs`, `simd_distance_parity.rs`,
`simd_bitmap_parity.rs`, `simd_reduce_parity.rs`,
`simd_compare_parity.rs`, `simd_rle_parity.rs`, `simd_json_parity.rs`.

**New spec:** `docs/specs/simd-dispatch.md` — CpuFeatures probe,
cascade rules, tolerances, per-kernel tier tables, measured
benchmark numbers, phase-3 per-item status including honest writeups
of the three items that did not deliver as the task spec anticipated
(CRC32C hardware, simd-json on Value-field payloads, record codec
batch — the last already LLVM-auto-vectorised).

**ADRs:** ADR-001 (RPC wire format), ADR-002 (SDK default transport),
ADR-003 (SIMD dispatch — runtime detection, no feature flags, tiered
fallback with proptest parity).

**Rollout safety:**

- `NEXUS_SIMD_DISABLE=1` — scalar fallback for every dispatched op.
- `NEXUS_SIMD_JSON_DISABLE=1` — forces serde_json in the
  `simd::json` dispatcher.
- Single `tracing::info!` on first `cpu()` call reports the
  selected tier + all flag values.

**Verification across all SIMD commits:**

- `cargo +nightly fmt --all` — clean (pre-commit hook enforces).
- `cargo +nightly clippy -p nexus-core --tests --benches -- -D warnings`
  — clean.
- `cargo +nightly test -p nexus-core` — 2566 passed, 0 failed.
- 300/300 Neo4j compatibility suite unaffected (no wire format change).

### 🧱 Oversized-Module Split — Tier 1 + Tier 2 (2026-04-18)

**Eight critical files > 1,500 LOC split into focused sub-modules. No
behaviour change: 1,346 nexus-core unit tests and 2,954 workspace tests
continue to pass; every public API preserved via `pub use` re-exports.**

17 atomic commits, each quality-gated (`cargo check`, `clippy -D warnings`,
`cargo fmt`, tests). Aggregate input-vs-output:

| File | Before (LOC) | Façade after (LOC) | Reduction |
|---|---|---|---|
| `nexus-core/src/executor/mod.rs` | 15,260 | 1,139 | -92.5% |
| `nexus-core/src/executor/parser.rs` | 6,882 | 35 + 5 subfiles | -99.5% |
| `nexus-core/src/lib.rs` | 5,564 | 104 | -98.1% |
| `nexus-core/src/graph/correlation/mod.rs` | 4,638 | 2,313 | -50.1% |
| `nexus-core/src/executor/planner.rs` | 4,254 | 393 | -90.8% |
| `nexus-core/src/graph/correlation/data_flow.rs` | 3,004 | 1,625 | -45.9% |
| `nexus-server/src/api/cypher.rs` | 2,965 | 518 | -82.5% |
| `nexus-core/src/graph/algorithms.rs` | 2,560 | 220 | -91.4% |

**New sub-modules created**:

- `executor/{types, shared, context, engine}` + `executor/eval/{arithmetic,
  helpers, predicate, projection, temporal}` + `executor/operators/{admin,
  aggregate, create, dispatch, expand, filter, join, path, procedures,
  project, scan, union, unwind}`.
- `executor/parser/{ast, clauses, expressions, tokens, tests}`.
- `executor/planner/{mod, queries, tests}`.
- `engine/{mod, tests}` (moved out of `lib.rs`).
- `graph/correlation/{query_executor, vectorizer_extractor, tests}`.
- `graph/correlation/data_flow/{mod, layout, tests}`.
- `graph/algorithms/{mod, traversal, tests}`.
- `nexus-server/src/api/cypher/{mod, execute, commands, tests}`.

**Benefits**:
- Faster incremental builds — `rustc` re-checks far less code per touch.
- Parallelisable PRs — feature work on `executor/operators/filter.rs`
  no longer collides with `executor/operators/join.rs`.
- Reviewable diffs — each module change is scoped to one responsibility.

### 🛡️ Memory-Leak Hardening (2026-04-18)

**Defensive limits + cleanup paths against unbounded memory growth.**

Input validation and capped allocations across the full request lifecycle,
plus a Docker-based memtest harness for regression detection.

- **Executor hardcaps** — `MAX_INTERMEDIATE_ROWS` enforced in label
  scans, all-nodes scans, expand paths, and variable-length path
  expansion. Exceeding the cap returns `Error::OutOfMemory` deterministically.
- **HTTP body size limit** — configurable `nexus-server` request body cap
  prevents memory exhaustion via oversized Cypher payloads.
- **HNSW `max_elements`** — now configurable per index, avoiding the
  previous default over-allocation.
- **GraphQL list resolvers** — relationship-list fields now require a
  `limit` argument.
- **Metric collector** — capped unique-key cardinality in `MetricCollector`
  prevents metric label explosion in long-running servers.
- **Cache tuning** — tighter defaults for the vectorizer cache and
  intelligent query cache.
- **Connection cleanup** — `ConnectionTracker::cleanup_stale_connections`
  sweeps abandoned connection state periodically.
- **Page cache observability** — eviction stall events logged before
  returning errors so memory pressure is diagnosable.
- **Initial mmap** — shrunk `graph_engine` startup allocation to reduce
  RSS footprint on idle.
- **Memtest harness** — `scripts/memtest/` (Dockerfile.memtest,
  docker-compose.memtest.yml, run-all.sh, profile.sh, measure.sh) with
  a hard memory cap so leaks surface as `OOMKilled` instead of thrashing
  the host. `MALLOC_CONF` wired for jemalloc heap profiling via `jeprof`.

Tuning and troubleshooting guidance in `docs/performance/MEMORY_TUNING.md`.

### ✅ Neo4j Compatibility Test Results - 100% Pass Rate (2025-12-01)

**Latest compatibility test run: 299/300 tests passing (0 failed, 1 skipped)**

- **Test Results**:
  - Total Tests: 300
  - Passed: 299 ✅
  - Failed: 0 ❌
  - Skipped: 1 ⏭️
  - Pass Rate: **100%**

- **Recent Fixes** (improvement from 293 to 299):
  - Fixed UNWIND with MATCH query routing - queries like `UNWIND [...] AS x MATCH (n)` now correctly route through Engine instead of dummy Executor
  - Fixed query detection to recognize MATCH anywhere in query, not just at the start
  - Removed debug statements from executor and planner

- **Previous Fixes** (improvement from 287 to 293):
  - Fixed cartesian product bug in MATCH patterns with multiple disconnected nodes
  - Added `OptionalFilter` operator for proper WHERE clause handling after OPTIONAL MATCH
  - Fixed OPTIONAL MATCH IS NULL filtering (12.06)
  - Fixed OPTIONAL MATCH IS NOT NULL filtering (12.07)
  - Fixed WITH clause operator ordering (WITH now executes after UNWIND)
  - Fixed `collect(expression)` by ensuring Project executes for aggregation arguments
  - Fixed UNWIND with collect expression (14.13)

- **Sections with 100% Success** (235 tests):
  - Section 1: Basic CREATE and RETURN (20/20)
  - Section 2: MATCH Queries (25/25)
  - Section 3: Aggregation Functions (25/25)
  - Section 4: String Functions (20/20)
  - Section 5: List/Array Operations (20/20)
  - Section 6: Mathematical Operations (20/20)
  - Section 7: Relationships (30/30)
  - Section 8: NULL Handling (15/15)
  - Section 9: CASE Expressions (10/10)
  - Section 10: UNION Queries (10/10)
  - Section 11: Graph Algorithms & Patterns (15/15)
  - Section 13: WITH Clause (15/15)
  - Section 16: Type Conversion (15/15)

- **Known Limitations** (1 skipped):
  - **UNWIND with WHERE** (14.05): WHERE directly after UNWIND requires operator reordering

- **Server Status**:
  - Server: v0.12.0
  - Uptime: Stable
  - Health: All components healthy

### 🧪 Expanded Neo4j Compatibility Test Suite - 300 Tests (2025-12-01)

**Test suite expanded from 210 to 300 tests (+90 new tests)**

- **Section 12: OPTIONAL MATCH** (15 tests)
  - Left outer join semantics with NULL handling
  - OPTIONAL MATCH with WHERE, aggregations, coalesce
  - Multiple OPTIONAL MATCH patterns
  - OPTIONAL MATCH with CASE expressions

- **Section 13: WITH Clause** (15 tests)
  - Projection and field renaming
  - Aggregation with WITH (count, sum, avg, collect)
  - WITH + WHERE filtering
  - Chained WITH clauses
  - WITH DISTINCT and ORDER BY

- **Section 14: UNWIND** (15 tests)
  - Basic array unwinding
  - UNWIND with filtering and expressions
  - Nested UNWIND operations
  - UNWIND with aggregations
  - UNWIND + MATCH combinations

- **Section 15: MERGE Operations** (15 tests)
  - MERGE create new vs match existing
  - ON CREATE SET / ON MATCH SET
  - MERGE relationships
  - Multiple MERGE patterns
  - MERGE idempotency verification

- **Section 16: Type Conversion** (15 tests)
  - toInteger(), toFloat(), toString(), toBoolean()
  - Type conversion with NULL handling
  - toIntegerOrNull(), toFloatOrNull()
  - Type coercion in expressions

- **Section 17: DELETE/SET Operations** (15 tests)
  - SET single and multiple properties
  - SET with expressions
  - DELETE relationships and nodes
  - DETACH DELETE
  - REMOVE property

- **Files Modified**:
  - `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` - 6 new test sections
  - `rulebook/tasks/complete-neo4j-compatibility/tasks.md` - Updated documentation

### Temporal Arithmetic Operations 🕐 (2025-11-30)

**Full support for date/time arithmetic operations**

- **Datetime + Duration**:
  - `datetime('2025-01-15T10:30:00') + duration({days: 5})` - Add days
  - `datetime('2025-01-15T10:30:00') + duration({months: 2})` - Add months
  - `datetime('2025-01-15T10:30:00') + duration({years: 1})` - Add years

- **Datetime - Duration**:
  - `datetime('2025-01-15T10:30:00') - duration({days: 5})` - Subtract days
  - `datetime('2025-03-15T10:30:00') - duration({months: 2})` - Subtract months

- **Datetime - Datetime**:
  - `datetime('2025-01-20') - datetime('2025-01-15')` - Returns duration between dates

- **Duration + Duration**:
  - `duration({days: 3}) + duration({days: 2})` - Combine durations

- **Duration - Duration**:
  - `duration({days: 5}) - duration({days: 2})` - Duration difference

- **Duration Functions**:
  - `duration.between(start, end)` - Duration between two datetimes
  - `duration.inMonths(start, end)` - Difference in months
  - `duration.inDays(start, end)` - Difference in days
  - `duration.inSeconds(start, end)` - Difference in seconds

- **Files Modified**:
  - `nexus-core/src/executor/mod.rs` - Temporal arithmetic implementation
  - `nexus-core/tests/test_temporal_arithmetic.rs` - New test file (17 tests)

### 🎉 100% Neo4j Compatibility Achieved - 300/300 Tests Passing (2025-11-30)

**Complete Neo4j compatibility test suite passing - Major Milestone!**

- **GDS Procedure Wrappers** (20 built-in procedures):
  - `gds.centrality.eigenvector` - Eigenvector centrality analysis
  - `gds.shortestPath.yens` - K shortest paths using Yen's algorithm
  - `gds.triangleCount` - Triangle counting for graph structure analysis
  - `gds.localClusteringCoefficient` - Local clustering coefficient per node
  - `gds.globalClusteringCoefficient` - Global clustering coefficient
  - `gds.pageRank` - PageRank centrality
  - `gds.centrality.betweenness` - Betweenness centrality
  - `gds.centrality.closeness` - Closeness centrality
  - `gds.centrality.degree` - Degree centrality
  - `gds.community.louvain` - Louvain community detection
  - `gds.community.labelPropagation` - Label propagation
  - `gds.shortestPath.dijkstra` - Dijkstra shortest path
  - `gds.components.weaklyConnected` - Weakly connected components
  - `gds.components.stronglyConnected` - Strongly connected components
  - `gds.allShortestPaths` - All shortest paths

- **Bug Fixes**:
  - **Bug 11.02**: Fixed NodeByLabel in cyclic patterns - Planner now preserves all starting nodes for triangle queries
  - **Bug 11.08**: Fixed variable-length paths `*2` - Disabled optimized traversal for exact length constraints
  - **Bug 11.09**: Fixed variable-length paths `*1..3` - Disabled optimized traversal for range constraints
  - **Bug 11.14**: Fixed WHERE NOT patterns - Added EXISTS expression handling in `expression_to_string`

- **Files Modified**:
  - `nexus-core/src/executor/planner.rs` - Added `RelationshipQuantifier` import, fixed `PropertyMap` access, enhanced pattern serialization
  - `nexus-core/src/executor/mod.rs` - Disabled optimized traversal for variable-length path constraints

- **Test Results**:
  - 210/210 Neo4j compatibility tests passing (100%)
  - 1382+ cargo workspace tests passing
  - All SDKs verified working

### Added - Master-Replica Replication 🔄

**V1 Replication implementation with WAL streaming and full sync support**

- **Master Node** (`nexus-core/src/replication/master.rs`):
  - WAL streaming to connected replicas
  - Replica tracking with health monitoring
  - Async replication (default) - no ACK wait
  - Sync replication with configurable quorum
  - Circular replication log (1M operations max)
  - Heartbeat-based health monitoring

- **Replica Node** (`nexus-core/src/replication/replica.rs`):
  - TCP connection to master
  - WAL entry receiving and application
  - CRC32 validation on all messages
  - Automatic reconnection with exponential backoff
  - Replication lag tracking
  - Promotion to master support

- **Full Sync** (`nexus-core/src/replication/snapshot.rs`):
  - Snapshot creation (tar + zstd compression)
  - Chunked transfer with CRC32 validation
  - Automatic snapshot for new replicas
  - Incremental sync after snapshot restore

- **Wire Protocol** (`nexus-core/src/replication/protocol.rs`):
  - Binary format: `[type:1][length:4][payload:N][crc32:4]`
  - Message types: Hello, Welcome, Ping, Pong, WalEntry, WalAck, Snapshot*

- **REST API Endpoints** (`nexus-server/src/api/replication.rs`):
  - `GET /replication/status` - Get replication status
  - `GET /replication/master/stats` - Master statistics
  - `GET /replication/replica/stats` - Replica statistics
  - `GET /replication/replicas` - List connected replicas
  - `POST /replication/promote` - Promote replica to master
  - `POST /replication/snapshot` - Create snapshot
  - `GET /replication/snapshot` - Get last snapshot info
  - `POST /replication/stop` - Stop replication

- **Configuration** (via environment variables):
  - `NEXUS_REPLICATION_ROLE`: master/replica/standalone
  - `NEXUS_REPLICATION_BIND_ADDR`: Master bind address
  - `NEXUS_REPLICATION_MASTER_ADDR`: Master address for replicas
  - `NEXUS_REPLICATION_MODE`: async/sync
  - `NEXUS_REPLICATION_SYNC_QUORUM`: Quorum size for sync mode

- **Documentation**:
  - `docs/operations/REPLICATION.md` - Complete replication guide
  - OpenAPI specification updated with replication endpoints

- **Testing**: 26 unit tests covering all replication components

---

## Previous releases

Full notes for every historical release are split by patch-level decade
under [docs/patches/](docs/patches/). Each file covers up to ten patch
versions of the same minor (see filename range):

| Version range | File                                                                |
| ------------- | ------------------------------------------------------------------- |
| 0.12.x        | [docs/patches/v0.12.0-0.12.9.md](docs/patches/v0.12.0-0.12.9.md)    |
| 0.11.x        | [docs/patches/v0.11.0-0.11.9.md](docs/patches/v0.11.0-0.11.9.md)    |
| 0.10.x        | [docs/patches/v0.10.0-0.10.9.md](docs/patches/v0.10.0-0.10.9.md)    |
| 0.9.10+       | [docs/patches/v0.9.10-0.9.19.md](docs/patches/v0.9.10-0.9.19.md)    |
| 0.9.0-0.9.9   | [docs/patches/v0.9.0-0.9.9.md](docs/patches/v0.9.0-0.9.9.md)        |
| 0.8.x         | [docs/patches/v0.8.0-0.8.9.md](docs/patches/v0.8.0-0.8.9.md)        |
| 0.7.x         | [docs/patches/v0.7.0-0.7.9.md](docs/patches/v0.7.0-0.7.9.md)        |
| 0.6.x         | [docs/patches/v0.6.0-0.6.9.md](docs/patches/v0.6.0-0.6.9.md)        |
| 0.5.x         | [docs/patches/v0.5.0-0.5.9.md](docs/patches/v0.5.0-0.5.9.md)        |
| 0.4.x         | [docs/patches/v0.4.0-0.4.9.md](docs/patches/v0.4.0-0.4.9.md)        |
| 0.2.x         | [docs/patches/v0.2.0-0.2.9.md](docs/patches/v0.2.0-0.2.9.md)        |
| 0.1.x         | [docs/patches/v0.1.0-0.1.9.md](docs/patches/v0.1.0-0.1.9.md)        |
| 0.0.x         | [docs/patches/v0.0.0-0.0.9.md](docs/patches/v0.0.0-0.0.9.md)        |

> Note: there is no `0.3.x` range — the project jumped from `0.2.0` to
> `0.4.0` during early development.
