# Proposal: phase8_encryption-at-rest-kms

## Why

The `KeyProvider` trait shipped in `phase8_encryption-at-rest` already accepts master keys from env vars and key files — acceptable for self-hosted deployments. Production cloud deployments need first-class KMS adapters: AWS KMS, GCP KMS, and HashiCorp Vault. Without these, customers either run with weaker key management or have to write their own provider.

## What Changes

- New module `crates/nexus-core/src/storage/crypto/kms/` with one file per provider:
  - `aws.rs` — `AwsKmsKeyProvider` using `aws-sdk-kms`.
  - `gcp.rs` — `GcpKmsKeyProvider` using `google-cloud-kms`.
  - `vault.rs` — `VaultKeyProvider` using HashiCorp Vault's KV v2 + transit secret engines.
- Each adapter calls the KMS at construction to fetch the master key (DEK pattern: KMS-wrapped data key), caches the result for the process lifetime, surfaces `ERR_KEY_KMS_FAILURE` with a structured cause on transient failures.
- Operator-facing config: `--kms-provider {aws,gcp,vault}` plus per-provider env vars / config keys.
- Integration tests that hit a local mock (`localstack` for AWS, GCP emulator for GCP, `vault dev` for Vault).

## Impact

- Affected specs: `docs/security/ENCRYPTION_AT_REST.md` § Key management.
- Affected code: new `crates/nexus-core/src/storage/crypto/kms/`.
- Breaking change: NO (additive providers).
- User benefit: turnkey cloud KMS integration; no customer-side glue.
