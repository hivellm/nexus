## 1. AWS KMS
- [x] 1.1 Add aws-sdk-kms dep, gate behind kms-aws feature — `aws-config` + `aws-sdk-kms` (both `behavior-version-latest` + `rustls`) added to workspace `Cargo.toml`; nexus-core declares them optional and rolls them up under the `kms-aws` feature. Default builds do not pull the SDK.
- [x] 1.2 Implement AwsKmsKeyProvider (DEK pattern: KMS-wrapped data key) — `crates/nexus-core/src/storage/crypto/kms/aws.rs`. Reads the wrapped DEK off disk, calls `kms:Decrypt` once at construction, caches the 32-byte plaintext in `Zeroizing<[u8; 32]>`, surfaces `ERR_KEY_KMS_CONFIG` / `ERR_KEY_KMS_FAILURE` / `ERR_KEY_KMS_BAD_LENGTH` on the failure paths.
- [x] 1.3 Integration test against localstack — `crates/nexus-core/tests/kms_aws_integration.rs`, `#[ignore]`-gated; the file's doc-comment carries the localstack provisioning recipe (`docker run localstack` + `aws kms encrypt`).

## 2. GCP KMS
- [x] 2.1 Add google-cloud-kms dep, gate behind kms-gcp feature — `google-cloud-kms` + `google-cloud-auth` + `google-cloud-gax` added to the workspace; rolled up under nexus-core's `kms-gcp` feature.
- [x] 2.2 Implement GcpKmsKeyProvider — `crates/nexus-core/src/storage/crypto/kms/gcp.rs`. Validates the `projects/.../cryptoKeys/...` resource path, calls Cloud-KMS `Decrypt` once via the gRPC client, caches the plaintext.
- [x] 2.3 Integration test against the GCP emulator — `crates/nexus-core/tests/kms_gcp_integration.rs`, `#[ignore]`-gated against the community-maintained google-cloud-kms emulator.

## 3. HashiCorp Vault
- [x] 3.1 Add vaultrs dep, gate behind kms-vault feature — `vaultrs = "0.7"` added to the workspace; rolled up under `kms-vault`.
- [x] 3.2 Implement VaultKeyProvider (transit secret engine) — `crates/nexus-core/src/storage/crypto/kms/vault.rs`. Reads the transit ciphertext (`vault:v<n>:...`) off disk, calls `transit/decrypt/<key>` once at construction, base64-decodes the response, caches the plaintext.
- [x] 3.3 Integration test against `vault dev` — `crates/nexus-core/tests/kms_vault_integration.rs`, `#[ignore]`-gated; the file's doc-comment carries the `vault server -dev` provisioning recipe.

## 4. Operator config
- [x] 4.1 Wire --kms-provider flag + per-provider config keys — `NEXUS_KMS_PROVIDER` env var routed through `resolve_encryption_config` → `resolve_kms` in `crates/nexus-server/src/config.rs`. Each provider has its own `resolve_kms_<provider>` helper with a `#[cfg(not(feature = "kms-<x>"))]` stub that fails fast with a clear "feature not built in" error. New `EncryptionSource::Kms { provider, label }` variant lands the resolved adapter on `/admin/encryption/status`.
- [x] 4.2 Document each provider's required env vars — `docs/security/ENCRYPTION_AT_REST.md` § "KMS adapters" rewritten with the per-provider env-var table, build matrix (`--features kms-aws|kms-gcp|kms-vault|kms`), DEK-pattern explainer, and `KmsError` taxonomy. CHANGELOG entry under `[1.15.0]` summarises the operator-facing surface.

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 5.1 Update or create documentation covering the implementation — `docs/security/ENCRYPTION_AT_REST.md` § "KMS adapters" + status header + follow-up table updated; CHANGELOG entry added.
- [x] 5.2 Write tests covering the new behavior — 13 KMS unit tests (`storage::crypto::kms::*`) covering the shared `KmsError` taxonomy + per-provider config-validation paths, 4 new server-side encryption tests covering the resolution branches + `EncryptionSource::Kms` JSON serialisation (9 total in the encryption_tests module), and 3 `#[ignore]`-gated integration tests for the live-network paths.
- [x] 5.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --lib storage::crypto` 45/45 green; `cargo +nightly test -p nexus-server --lib encryption_tests` 9/9 green; per-feature unit tests `--features kms-aws` 7/7 green, `--features kms-gcp` 8/8 green, `--features kms-vault` 9/9 green; clippy `-p nexus-core -p nexus-server --all-targets -- -D warnings` clean on default features and on `--features kms`.
