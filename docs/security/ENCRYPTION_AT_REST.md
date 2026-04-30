# Encryption at rest

> **Status (2026-04-30)**: cryptographic core shipped under
> `phase8_encryption-at-rest`; online key rotation shipped under
> `phase8_encryption-at-rest-rotation`; AWS / GCP / Vault KMS
> adapters shipped under `phase8_encryption-at-rest-kms`; WAL
> append + replay shipped under `phase8_encryption-at-rest-wal`.
> Storage-layer hooks for the LMDB catalog, mmap-backed record
> stores, B-tree / Tantivy / HNSW indexes, and the migration CLI
> are tracked under separate follow-up tasks listed at the bottom
> of this document. The contracts below are stable; the follow-ups
> consume them without changing any public API.

Encryption at rest means every byte the engine writes to disk is
ciphertext, and every byte it reads back is decrypted before
reaching the executor. It's a compliance gate for SOC2, FedRAMP,
HIPAA, GDPR, and PCI-DSS workloads — every comparable engine
(Neo4j Enterprise, Aura, ArangoDB Enterprise, Memgraph Enterprise)
ships it.

## Threat model

### What encryption at rest protects against

| Threat | Outcome with EaR |
|---|---|
| Drive theft from a decommissioned server | Ciphertext is useless without the master key. |
| Cold-snapshot exfiltration (volume snapshot leaked, backup tarball stolen) | Same. |
| Physical-media decommissioning without a wipe procedure | Drives can be returned / destroyed safely. |
| A backup operator with read access to volume snapshots but no KMS access | Same. |

### What encryption at rest does NOT protect against

| Threat | Why EaR can't help |
|---|---|
| Runtime memory dumps | Master key + per-database keys live in process memory. Mitigate at the OS level (lock the binary's memory pages, sanitise core-dump policy). |
| Hostile root on the running host | Nothing engine-side can prevent it. Mitigate with a hardened OS + auditd + EDR. |
| A rogue DBA with `SELECT *` privileges | EaR is transparent to anyone with valid credentials. Mitigate with RBAC + audit logs. |
| Side-channel timing attacks on the AEAD | AES-GCM is constant-time on every CPU Nexus targets (AES-NI / ARMv8 crypto extensions); no engineering action required. |
| In-flight network exfiltration | Out of scope. Use TLS on the listener (see `docs/operations/KUBERNETES.md` § TLS). |

## Architecture

```
                        ┌──────────────────┐
                        │   KeyProvider    │
                        │  (env / file /   │
                        │   AWS / GCP /    │
                        │   Vault)         │
                        └─────────┬────────┘
                                  │ master key (32 B)
                                  ▼
                        ┌──────────────────┐
                        │  HKDF-SHA-256    │
                        │  per-database    │
                        │  key derivation  │
                        └─────────┬────────┘
                                  │ 1 key per db × epoch
                                  ▼
        ┌─────────────────────────────────────────────────┐
        │              PageCipher (AES-256-GCM)           │
        └─────────────────────────────────────────────────┘
                                  │
              ┌───────────────────┼─────────────────────┐
              ▼                   ▼                     ▼
        ┌─────────────┐   ┌──────────────┐    ┌─────────────────┐
        │ catalog +   │   │  WAL + index │    │ EncryptedPage-  │
        │ record-store│   │  files       │    │ Stream wrappers │
        │ pages       │   │              │    │ (page header +  │
        │             │   │              │    │  AAD-bound AEAD)│
        └─────────────┘   └──────────────┘    └─────────────────┘
```

The four files in [`crates/nexus-core/src/storage/crypto/`](../../crates/nexus-core/src/storage/crypto/)
are:

| File | Purpose |
|---|---|
| `key_provider.rs` | `KeyProvider` trait, `EnvKeyProvider`, `FileKeyProvider`. |
| `kdf.rs` | HKDF-SHA-256 per-database key derivation with rotation epoch. |
| `aes_gcm.rs` | AES-256-GCM page cipher with deterministic per-page nonce. |
| `encrypted_file.rs` | `EncryptedPageStream` — the seam storage hooks plug into. |

## Cryptographic choices

| Decision | Choice | Why |
|---|---|---|
| AEAD | AES-256-GCM | NIST SP 800-38D; constant-time on AES-NI / ARMv8 crypto extensions; default in every comparable engine. |
| Key length | 256 bits | Resistance to a hypothetical Grover attack on a quantum adversary; required by FIPS 140-3 for "192-bit security level" deployments. |
| Nonce length | 96 bits (NIST default) | Required by AES-GCM. |
| KDF | HKDF-SHA-256 (RFC 5869) | Extract-then-expand provides domain separation between the master key and each per-database key. |
| Nonce derivation | `(file_id ‖ page_offset ‖ generation)`, packed big-endian | Deterministic per-page; the 32-bit generation counter prevents reuse on overwrites. AES-GCM is **catastrophically broken** under nonce reuse. |
| Key zeroisation | `zeroize::Zeroizing` on every secret | Wipes memory on `Drop`. |
| Domain-separation tag | `nexus-encryption-at-rest-v1` | Bumping the constant invalidates every previously-derived key without requiring a master-key rotation. |

### Per-page nonce layout

```
  bytes [0..2]   = file_id        (16 bits, network-order)
  bytes [2..8]   = page_offset    (low 48 bits, network-order)
  bytes [8..12]  = generation     (32 bits, network-order)
```

Rationale: the `(file_id, page_offset, generation)` triple is
unique by construction. `file_id` is a stable enum
([`crypto::FileId`](../../crates/nexus-core/src/storage/crypto/encrypted_file.rs)),
`page_offset` is the file offset of the page (48 bits ⇒ 256 TiB
addressable per file), and `generation` is monotonically
incremented on every overwrite of the same page. Storage hooks
must never construct a `PageNonce` without bumping the generation.

## Key management

### Master key sources

#### 1. `NEXUS_DATA_KEY` env var (default)

```bash
export NEXUS_DATA_KEY="$(openssl rand -hex 32)"
nexus-server --encrypt-at-rest
```

`EnvKeyProvider` reads the variable **once at construction time**.
A hostile process that later sets the var cannot influence the
key.

#### 2. Key file

```bash
openssl rand -out /etc/nexus/master.key 32
chmod 0600 /etc/nexus/master.key
nexus-server --encrypt-at-rest --key-file /etc/nexus/master.key
```

`FileKeyProvider` accepts either 32 raw bytes or a 64-character
hex string. It enforces `0600` perms on Unix; on Windows, the
permission check is best-effort (`tracing::warn!`) and operators
should rely on filesystem ACLs.

#### 3. KMS adapters

AWS KMS, GCP KMS, and HashiCorp Vault adapters live in
[`crates/nexus-core/src/storage/crypto/kms/`](../../crates/nexus-core/src/storage/crypto/kms/)
and plug into the same `KeyProvider` trait. Each adapter is
gated behind its own Cargo feature so default builds do not pay
the SDK transitive-dep cost; operators opt in at build time.

**DEK pattern.** Each adapter holds a reference to a KMS-owned
**key encryption key** (KEK) and a blob of **wrapped data key**
(DEK) that the operator generated once via the KMS' encrypt
call and stored on disk (or in an env var). At construction
time the adapter calls the KMS once to unwrap the DEK, caches
the 32-byte plaintext for the process lifetime, and returns it
from `KeyProvider::master_key` thereafter. The KMS is never on
the hot path — a transient KMS outage after boot does not
affect serving traffic.

**Build:**

```bash
# All three providers compiled in:
cargo build --release --features kms

# Per-provider:
cargo build --release --features kms-aws
cargo build --release --features kms-gcp
cargo build --release --features kms-vault
```

**Required env vars per provider** (in addition to
`NEXUS_ENCRYPT_AT_REST=1` and `NEXUS_KMS_PROVIDER`):

| Provider | Env vars |
|---|---|
| `aws` | `NEXUS_KMS_WRAPPED_DEK_FILE` (path to the KMS ciphertext blob), `NEXUS_KMS_AWS_REGION` (optional, defers to SDK chain), `NEXUS_KMS_AWS_KEY_ID` (optional alias/ARN for log readability), `NEXUS_KMS_AWS_ENDPOINT` (optional, for localstack). AWS credential discovery follows the [SDK default chain](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credentials.html) — `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY`, IAM role on EC2/EKS, etc. |
| `gcp` | `NEXUS_KMS_GCP_KEY_NAME` (full `projects/.../cryptoKeys/...` path), `NEXUS_KMS_WRAPPED_DEK_FILE`, `NEXUS_KMS_GCP_ENDPOINT` (optional, for emulator). Auth uses the GCP default credential chain — `GOOGLE_APPLICATION_CREDENTIALS`, GKE Workload Identity, or instance metadata. |
| `vault` | `NEXUS_KMS_VAULT_ADDR`, `NEXUS_KMS_VAULT_TOKEN`, `NEXUS_KMS_VAULT_KEY` (transit key name), `NEXUS_KMS_WRAPPED_DEK_FILE`, optional `NEXUS_KMS_VAULT_MOUNT` (defaults to `transit`), `NEXUS_KMS_VAULT_NAMESPACE` (Vault Enterprise), `NEXUS_KMS_VAULT_INSECURE_SKIP_VERIFY` (dev only). The adapter does not do `auth/login` itself — operators mint a token out-of-band (CLI / AppRole helper / sidecar) and inject it via env. |

**Operator setup recipes** for one-shot DEK provisioning live as
doc-comments at the top of each adapter source file
([`aws.rs`](../../crates/nexus-core/src/storage/crypto/kms/aws.rs),
[`gcp.rs`](../../crates/nexus-core/src/storage/crypto/kms/gcp.rs),
[`vault.rs`](../../crates/nexus-core/src/storage/crypto/kms/vault.rs)).

**Errors.** Every adapter surfaces failures through the shared
`KmsError` taxonomy:

| Code | When |
|---|---|
| `ERR_KEY_KMS_CONFIG` | Missing / malformed operator config (no wrapped DEK, empty token, bad key path). |
| `ERR_KEY_KMS_FAILURE` | KMS request itself failed — network error, auth rejection, throttling, KEK not found. |
| `ERR_KEY_KMS_BAD_LENGTH` | KMS returned a payload whose plaintext was not exactly 32 bytes. Either the wrapped blob was generated for a different scheme or it was corrupted. |

All three errors fail fast at boot — the server never silently
falls through to plaintext when the operator asked for a KMS.

**Integration tests.** Each adapter ships an `#[ignore]`-gated
integration test that runs against a local mock (localstack /
GCP KMS emulator / `vault dev`); see
[`crates/nexus-core/tests/kms_*_integration.rs`](../../crates/nexus-core/tests/)
for the recipes.

### Per-database derivation

Every database gets a unique key derived from the master:

```rust
let m = MasterKey::new(/* 32 bytes from KeyProvider */);
let k = derive_database_key(&m, "tenant-acme", /* epoch */ 0)?;
```

Properties of the derivation:

* Deterministic — same master + db name + epoch produces the same
  derived key.
* Distinct — changing any one input changes the output.
* Forward-secure — compromising one database's derived key does
  not reveal the master.
* Rotatable — bumping `epoch` derives an independent key without
  disturbing other databases.

### Online key rotation

Shipped under `phase8_encryption-at-rest-rotation` in
[`crates/nexus-core/src/storage/crypto/rotation.rs`](../../crates/nexus-core/src/storage/crypto/rotation.rs).
Contract:

1. Operator derives the new per-database key
   (`derive_database_key(master, db, new_epoch)`).
2. [`EncryptedPageStream::install_secondary`](../../crates/nexus-core/src/storage/crypto/encrypted_file.rs)
   installs the **previous** epoch's cipher as the read-fallback
   key.
3. The stream's primary cipher is rebuilt under the new epoch's
   key.
4. [`RotationRunner::run`](../../crates/nexus-core/src/storage/crypto/rotation.rs)
   walks every page in `(file_id, page_offset)` ascending order via
   the [`PageStore`] trait, decrypts under whichever key works,
   and re-encrypts under the primary if the source was the
   secondary. Idempotent on already-primary pages.
5. Once the runner returns, the operator calls
   [`EncryptedPageStream::clear_secondary`] to drop the old key
   out of memory.

Read traffic during the window pays one extra failed AEAD probe
per page that has not yet been rotated; cost is bounded by the
runner's progress. Writes always use the primary regardless.

The runner reports progress through a [`RotationCheckpoint`]
(serde-serialisable so the operator can persist it across
restarts) and accepts a resume cursor — recovery is "load the
last checkpoint, call `RotationRunner::run(checkpoint)`, runner
skips every page ≤ the cursor".

Throttling: `RotationRunnerConfig::byte_budget_per_second`
(default 64 MiB/s) caps the re-encryption rate so live read /
write traffic is never starved.

The `PageStore` trait is the seam the runner walks; the
in-memory implementation is shipped today, and the storage-hooks
follow-up wires the LMDB catalog + record stores + WAL + indexes
through their own `PageStore` impls.

Two-key window is bounded by the slowest re-encrypt pass on
disk; for a 1 TB database on NVMe at 64 MiB/s expect ~4.5 hours.

## Activation

When the storage hooks ship, the activation path is:

```bash
# Fresh database, encrypted from day one.
nexus-server --encrypt-at-rest

# Existing un-encrypted database — one-shot migration.
nexus admin encrypt-database default
```

Mixed mode (some files encrypted, others plaintext) is rejected
on startup with a clear error so an operator who half-migrated a
deployment notices immediately.

### WAL encryption (v3 frame format)

Shipped under `phase8_encryption-at-rest-wal` in
[`crates/nexus-core/src/wal/mod.rs`](../../crates/nexus-core/src/wal/mod.rs).
Encrypted WAL files use a new v3 frame:

```text
  [magic:1 = 0x00]              — shared with v2; tells the reader to consult the algo byte
  [algo:1 = 0x03]               — `Aes256GcmCrc32C`; dispatches to the v3 decoder
  [type:1]                      — `WalEntryType`
  [plain_len:4 LE]              — original payload length
  [crc_plain:4 LE]              — CRC32C over the plaintext (end-to-end)
  [ciphertext_with_tag: plain_len + 16]   — AES-256-GCM seal over the bincoded entry
```

Total frame length: `27 + plain_len`.

**AAD** (additional authenticated data) covers
`[type, plain_len_le4, crc_plain_le4, frame_offset_le8]` — 17
bytes. Binding the offset means a tamperer who relocates a frame
to a different position triggers an AEAD failure. Binding the
plaintext CRC means a tamperer who flips bits in the ciphertext
*and* the CRC field still cannot fake a valid frame.

**Nonce derivation**: `PageNonce::new(file_id = FileId::Wal,
page_offset = frame_offset_in_file, generation = 1)`. Nonce
uniqueness is guaranteed by the WAL's append-only invariant: each
frame gets a unique offset between truncations.

**WAL key rotation contract**: a `Wal::truncate()` resets frame
offsets to `PAGE_HEADER_LEN`. Re-using the same database key
across a truncate would reuse `(key, nonce)` for fresh frames —
catastrophic for AES-GCM. Operators **must** pair every WAL
truncate with a key rotation; the rotation runner shipped under
`phase8_encryption-at-rest-rotation` already coordinates with the
checkpoint epoch, so the production code path automatically gets
this guarantee. Test code may truncate without rotating — see
`v3_append_then_replay_after_truncate_starts_fresh_offsets` for the
shape, and the WAL test fixture `make_encrypted_wal` for the
constructor pattern.

**On-disk header**: encrypted WAL files start with a 16-byte
`NXCP` page header (`FileId::Wal`, generation `1`) so the boot
inventory scanner classifies them as `Encrypted`. The header is
written by `Wal::with_cipher` on file creation, validated on
file reopen, and rewritten by `truncate()` — the file is always a
valid encrypted-WAL file for the inventory's purposes.

**Replay tolerance**:

| Failure | Outcome |
|---|---|
| Short read mid-frame body | Treated as truncation: file is shrunk to the previous frame boundary, recover returns the entries it could decode. Parity with v1/v2 CRC-mismatch behaviour. |
| AEAD failure on a frame whose body extends to EOF | Treated as truncation (kill-9 mid-write produces this shape). |
| AEAD failure on a frame followed by more frames | `ERR_WAL_AEAD` — wholesale tamper or wrong key; recover refuses to continue. |
| Plaintext CRC mismatch after successful AEAD | `ERR_WAL_CRC` — hard integrity failure (the ciphertext authenticated under the AAD-bound CRC, but the recovered plaintext does not match). |
| `with_cipher` on an existing plaintext WAL | `ERR_WAL_HEADER` — refuse to open; the operator forgot to rotate the WAL alongside the encryption flag flip. |

### Mixed-mode detection (boot invariant)

Shipped under `phase8_encryption-at-rest-storage-hooks` at
[`crates/nexus-core/src/storage/crypto/inventory.rs`](../../crates/nexus-core/src/storage/crypto/inventory.rs).
Runs unconditionally on every server boot, before the executor
opens any record store. The scanner walks the data directory,
reads the first 16 bytes of each regular file, and classifies
the file by the EaR magic (`0x4E58_4350`):

| State | Recovered from disk |
|---|---|
| `Empty` | File is zero-byte or shorter than the page header. No opinion — fresh boot legitimately produces empty bootstrap files. |
| `Plaintext` | First 16 bytes do not match the EaR magic. The file has not been written through the encrypted page stream. |
| `Encrypted` | First 16 bytes parse as a valid `PageHeader`. The recovered `(file_id, generation)` lands in the operator log. |

Decision matrix:

| `enabled` | plaintext files | encrypted files | Outcome |
|---|---|---|---|
| any | ≥ 1 | ≥ 1 | `ERR_ENCRYPTION_MIXED_MODE` — refuse to boot |
| `true` | 0 | any | OK (uniform encrypted; expected state) |
| `true` | ≥ 1 | 0 | `ERR_ENCRYPTION_NOT_INITIALIZED` — flag flipped on without running the migration verb |
| `false` | any | 0 | OK (uniform plaintext; pre-phase-8 deployment) |
| `false` | 0 | ≥ 1 | `ERR_ENCRYPTION_UNEXPECTED_ENCRYPTED` — flag flipped off would feed ciphertext to the executor |

Recovered counts (not paths) ship over
`GET /admin/encryption/status` under the new `inventory` field:

```json
{
  "enabled": true,
  "source": { "kind": "kms", "provider": "vault", "label": "..." },
  "fingerprint": "nexus:abcd1234efgh5678",
  "inventory": { "empty": 0, "plaintext": 0, "encrypted": 12 },
  "storage_surfaces": [],
  "schema_version": 1
}
```

Per-file paths are written to the operator log line at boot
(via `tracing::info!`) when an error fires, but never sent over
the network — they leak filesystem layout to a remote caller.

## Performance expectations

AES-256-GCM with AES-NI runs at ~3-5 GB/s per core on modern
x86_64. For Nexus's 8 KiB page granularity, the per-page overhead
is dominated by the AEAD tag append (~128 ns) and the nonce
derivation (~10 ns). Targets:

| Workload | Plaintext baseline | EaR target | Acceptable overhead |
|---|---|---|---|
| Sequential bulk write | engine-bound | engine-bound | < 5 % |
| Random page read | page-cache-bound | page-cache-bound | < 10 % |
| Mixed read / write | mixed | mixed | < 15 % |

The 15 % ceiling is the proposal's commitment; benchmarks
measuring against the pre-EaR baseline land alongside the
storage-hook follow-up.

## Operational checklist

| Step | Why |
|---|---|
| Provision the master key in your KMS, never on a developer laptop | Avoids the master key ever touching consumer hardware. |
| Restrict `NEXUS_DATA_KEY` / `--key-file` access to the `nexus` system user only | A reader of the env / file is equivalent to a reader of every encrypted byte. |
| Enable core-dump suppression (`ulimit -c 0`) on the runtime user | The master key is in memory; a core dump leaks it. |
| Disable `swapoff` or rely on encrypted swap | Unencrypted swap is equivalent to unencrypted disk for the lifetime of paged-out memory. |
| Audit the rotation cadence | NIST recommends rotating data-encryption keys at most annually; per-database epochs make it cheap. |
| Test the restore path on the passive standby | The encrypted backup needs the right key; verify before you need to. |

## Related work and follow-ups

| Task | Status | What it adds |
|---|---|---|
| `phase8_encryption-at-rest` | **shipped** | Crypto core: `KeyProvider`, KDF, AES-GCM page cipher, `EncryptedPageStream`. 36 unit tests. |
| `phase8_encryption-at-rest-storage-hooks` | **partial** | Boot-time mixed-mode invariant scanner shipped at [`crates/nexus-core/src/storage/crypto/inventory.rs`](../../crates/nexus-core/src/storage/crypto/inventory.rs). Walks the data directory at boot, classifies every file by its first page header (`Empty` / `Plaintext` / `Encrypted`), refuses to start when the on-disk state contradicts the encryption flag (mixed-mode, encrypted-files-with-flag-off, plaintext-files-with-flag-on). Result surfaces on `/admin/encryption/status` as a counts-only inventory summary. Actual page-stream wiring into the LMDB catalog, mmap-backed record stores, and page-cache buffer pool is blocked on a storage-layer refactor (LMDB has no engine-side page hook; record stores mutate `MmapMut` in place; the page cache has no real disk backing yet) — tracked in a follow-up architecture task. |
| `phase8_encryption-at-rest-wal` | **shipped** | WAL append + replay encrypted via a v3 frame format (`Aes256GcmCrc32C` algo). v3 layout: `[magic:1=0x00][algo:1=0x03][type:1][plain_len:4][crc_plain:4][ciphertext_with_tag: plain_len + 16]`. Plaintext CRC32C is bound into the AAD alongside the frame's file offset, so a tamperer who relocates a frame triggers an AEAD failure. Encrypted WAL files start with a 16-byte `NXCP` page header so the boot inventory scanner classifies them as `Encrypted`. Recover tolerates kill-9 truncation by treating an EOF-aligned AEAD failure as the truncation point (parity with the existing CRC-mismatch behaviour); mid-WAL AEAD failures surface `ERR_WAL_AEAD`. v1/v2 plaintext frames continue to replay byte-for-byte unchanged on the plaintext path. 10 new v3 tests covering round-trip, leak audit, wrong-key detection, mid-WAL tamper, trailing-frame truncation, plaintext / encrypted constructor mismatch, truncate preserves header, and post-truncate replay. See `crates/nexus-core/src/wal/mod.rs`. |
| `phase8_encryption-at-rest-indexes` | **partial** | R-tree shipped via [`EncryptedFilePageStore`](../../crates/nexus-core/src/index/rtree/encrypted_store.rs) (12 unit tests; parallel to the unencrypted `FilePageStore`; slot 8224 B). B-tree is in-memory today (no on-disk format to encrypt); Tantivy needs a custom `Directory` adapter; HNSW (`hnsw_rs`) lacks a streaming-IO seam. The R-tree pattern is the template the others adopt as their IO seams land. |
| `phase8_encryption-at-rest-kms` | **shipped** | AWS KMS (`aws-sdk-kms`), GCP KMS (`google-cloud-kms`), and HashiCorp Vault transit (`vaultrs`) adapters in [`crates/nexus-core/src/storage/crypto/kms/`](../../crates/nexus-core/src/storage/crypto/kms/). Each behind its own Cargo feature (`kms-aws` / `kms-gcp` / `kms-vault`); operator config via `NEXUS_KMS_PROVIDER` + per-provider env vars. 24 new tests (13 unit-tested config-validation paths + 8 unit + 3 ignored-by-default integration tests against localstack / GCP KMS emulator / `vault dev`). |
| `phase8_encryption-at-rest-rotation` | **shipped** | Online key rotation with two-key window. `EncryptedPageStream::install_secondary` + `RotationRunner` + `PageStore` trait + checkpoint + throttle. 9 unit tests. |
| `phase8_encryption-at-rest-cli` | follow-up | `nexus admin encrypt-database` / `rotate-key`. |

## Cross-references

- [`crates/nexus-core/src/storage/crypto/`](../../crates/nexus-core/src/storage/crypto/) — implementation.
- [`AUTHENTICATION.md`](./AUTHENTICATION.md) — runtime auth (orthogonal to EaR; protects different threats).
- [`SECURITY_AUDIT.md`](./SECURITY_AUDIT.md) — broader security review.
- [RFC 5869 — HKDF](https://datatracker.ietf.org/doc/html/rfc5869).
- [NIST SP 800-38D — AES-GCM](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-38d.pdf).
