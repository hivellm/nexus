# Proposal: phase8_encryption-at-rest-cli

## Why

Operators need a CLI surface for the encryption-at-rest lifecycle: migrate an existing un-encrypted database to encrypted, rotate keys, and verify status. Without these, every operation requires restarting the server with different flags — error-prone for SOC2 audits.

## What Changes

- New CLI subcommands:
  - `nexus admin encrypt-database <name>` — one-shot migration from un-encrypted to encrypted. Reads the master key from the configured `KeyProvider`, walks every page, encrypts in place. Refuses to run on a database that's already encrypted.
  - `nexus admin rotate-key --database <name>` — drives the online rotation runner from `phase8_encryption-at-rest-rotation`.
  - `nexus admin encryption-status [--database <name>]` — reports per-database encryption state, key epoch, rotation progress.
- Server flag `--encrypt-at-rest` to provision a fresh database encrypted from day one.
- Mixed-mode rejection on startup with a clear error pointing at the migration command.

## Impact

- Affected specs: `docs/security/ENCRYPTION_AT_REST.md` § "Activation".
- Affected code: `crates/nexus-cli/src/commands/admin.rs`, `crates/nexus-server/src/main.rs`.
- Breaking change: NO.
- User benefit: turnkey lifecycle ops.
