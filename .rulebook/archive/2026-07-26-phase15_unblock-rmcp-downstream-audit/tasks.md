# Tasks: phase15_unblock-rmcp-downstream-audit

RESOLVED BY SUPERSESSION. This task's remediation (delete the dead `rmcp` line
from `crates/nexus-protocol/Cargo.toml` + republish) was overtaken by a larger
change: `nexus-protocol` was **deleted as a crate entirely** (commit a72b5e43,
2026-07-25 — the Thunder dissolution), which removed the rmcp declaration, the
README claim, and the whole leak path from the source tree. `nexus-graph-sdk`
now depends only on the published `thunder-rpc` crate — zero rmcp, zero
`nexus-protocol` (verified: `grep rmcp sdks/rust` → no matches). Issue #28 was
closed 2026-07-21 (COMPLETED). The residual — publishing a new SDK version so
crates.io stops serving the vulnerable 2.5.0 — is carried by the in-flight
`release/3.0.0` (the current source is still versioned 2.5.0; crates.io cannot
overwrite 2.5.0, so the fix ships when 3.0.0 publishes).

## 1. Remove the dead dependency
- [x] 1.1 Re-confirmed: `grep -rn rmcp crates/nexus-protocol/` → zero hits (the crate directory no longer exists); `grep rmcp sdks/rust` → no matches
- [x] 1.2 Superseded: the entire `crates/nexus-protocol/` crate was deleted (a72b5e43), removing line 37 and the crate itself; root `Cargo.toml` rmcp pin left untouched (serves `nexus-server`, phase16)
- [x] 1.3 Verified in source: `nexus-graph-sdk` depends only on `thunder-rpc` (`sdks/rust/Cargo.toml`), no `nexus-protocol`/`rmcp` in the tree. NOTE: crates.io still serves the pre-dissolution 2.5.0 (`nexus-graph-sdk 2.5.0 → nexus-protocol 2.5.0 → rmcp ^0.8.1`) until 3.0.0 publishes
- [x] 1.4 Superseded: `crates/nexus-protocol/README.md` was deleted with the crate — no stale rmcp claim remains

## 2. Release
- [ ] 2.1 Carried by `release/3.0.0`: source SDK is still `2.5.0`; bump to `3.0.0` happens in the release, not this task
- [ ] 2.2 Carried by `release/3.0.0`: `sdks/rust/CHANGELOG.md` entry for the release that drops `nexus-protocol`/rmcp from the SDK graph
- [ ] 2.3 Carried by `release/3.0.0`: `cargo publish` of the clean `nexus-graph-sdk` — outward-facing, credential-gated, requires explicit user authorization
- [x] 2.4 Issue #28 already CLOSED (2026-07-21, COMPLETED). Downstream `cargo audit` on the Nexus path actually clears once 3.0.0 publishes (documented above); no further action on the issue

## 3. Tail (docs + tests — waived via tailWaiver: no code shipped by this superseded task)
- [ ] 3.1 Update or create documentation covering the implementation
- [ ] 3.2 Write tests covering the new behavior
- [ ] 3.3 Run tests and confirm they pass

<!-- tail-waiver: Resolved by supersession: the nexus-protocol crate was deleted entirely (commit a72b5e43, Thunder dissolution), removing the rmcp declaration, the README claim, and the whole leak path from source — nexus-graph-sdk now depends only on thunder-rpc (zero rmcp). Issue #28 is CLOSED (2026-07-21). No code shipped by this task, so docs/tests tail (3.1-3.3) has nothing to cover. Residual — republishing the clean SDK so crates.io stops serving the vulnerable 2.5.0 — is carried by the in-flight release/3.0.0 (crates.io cannot overwrite 2.5.0; publish is outward-facing + credential-gated); tracked in project memory project-phase15-rmcp-residual-publish. -->
