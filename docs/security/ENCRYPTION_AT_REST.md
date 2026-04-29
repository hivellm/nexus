# Encryption at rest

> **Status (2026-04-29)**: cryptographic core shipped under
> `phase8_encryption-at-rest`. Storage-layer hooks (catalog,
> record stores, WAL, indexes), KMS adapters (AWS / GCP / Vault),
> CLI migration, and online key rotation are tracked under
> separate follow-up tasks listed at the bottom of this document.
> The contracts below are stable; the follow-ups consume them
> without changing any public API.

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

#### 3. KMS adapters (follow-up)

AWS KMS, GCP KMS, and HashiCorp Vault adapters are tracked
under `phase8_encryption-at-rest-kms`. They plug into the same
`KeyProvider` trait — adding one is a ~150 LOC change with
docs, no engine-side rewiring.

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

### Online key rotation (follow-up)

Tracked under `phase8_encryption-at-rest-rotation`. The contract:

1. Operator increments the rotation epoch via the CLI.
2. The runner derives the new per-database key, holds both the
   old and new keys in memory.
3. Each page is re-encrypted in the background, lowest-offset
   first; reads probe new key first then old key on `ERR_BAD_KEY`.
4. Once every page is re-encrypted, the old key drops out of
   memory.

The two-key window is bounded by the slowest re-encrypt pass on
disk; for a 1 TB database on NVMe expect minutes.

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
| `phase8_encryption-at-rest-storage-hooks` | follow-up | Wire the page stream into LMDB catalog + record stores + page cache. |
| `phase8_encryption-at-rest-wal` | follow-up | WAL append + replay through the page stream. |
| `phase8_encryption-at-rest-indexes` | follow-up | B-tree, full-text, KNN, R-tree. |
| `phase8_encryption-at-rest-kms` | follow-up | AWS KMS, GCP KMS, Vault adapters. |
| `phase8_encryption-at-rest-rotation` | follow-up | Online key rotation with two-key window. |
| `phase8_encryption-at-rest-cli` | follow-up | `nexus admin encrypt-database` / `rotate-key`. |

## Cross-references

- [`crates/nexus-core/src/storage/crypto/`](../../crates/nexus-core/src/storage/crypto/) — implementation.
- [`AUTHENTICATION.md`](./AUTHENTICATION.md) — runtime auth (orthogonal to EaR; protects different threats).
- [`SECURITY_AUDIT.md`](./SECURITY_AUDIT.md) — broader security review.
- [RFC 5869 — HKDF](https://datatracker.ietf.org/doc/html/rfc5869).
- [NIST SP 800-38D — AES-GCM](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-38d.pdf).
