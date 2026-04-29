# Proposal: phase8_encryption-at-rest-rotation

## Why

NIST SP 800-57 recommends rotating data-encryption keys at most annually. The KDF in `phase8_encryption-at-rest` already supports per-database epochs — the missing piece is the **online** rotation runner that re-encrypts every page in the background while the server keeps serving traffic.

## What Changes

- Two-key window: during rotation, the server holds both the old (epoch N) and new (epoch N+1) per-database keys.
- Background runner that walks every page in lowest-offset order, decrypts under the old key, re-encrypts under the new key, and bumps the page generation counter.
- Read path probes the new key first; on `ERR_BAD_KEY`, falls back to the old key. Both succeed during the window.
- Once the runner completes, the old key drops out of memory.
- Progress reporting: `nexus_cluster_rotation_pages_total` / `_pages_rotated_total`.

## Impact

- Affected specs: `docs/security/ENCRYPTION_AT_REST.md` § "Online key rotation".
- Affected code: new `crates/nexus-core/src/storage/crypto/rotation.rs`.
- Breaking change: NO.
- User benefit: cleared NIST compliance gate; no downtime during rotation.
