# Third-Party Notices

This file lists external attribution required by the Apache License,
Version 2.0 for code, test corpora, and conventions vendored or
adapted into the Nexus repository. Each entry names the upstream
source, the license, and the location inside this repo.

## openCypher TCK conventions (test format only)

The Gherkin `.feature` shape and step grammar used by Nexus's
`crates/nexus-core/tests/tck/spatial/*.feature` corpus mirrors the
upstream openCypher TCK conventions:

> Source: <https://github.com/opencypher/openCypher>
> License: Apache License, Version 2.0
> Copyright: "Neo4j" — Neo4j Sweden AB and the openCypher community

The actual `.feature` files under `crates/nexus-core/tests/tck/spatial/`
are **Nexus-authored**; the upstream openCypher distribution does not
ship a spatial corpus (verified 2026-04-28 — see
`crates/nexus-core/tests/tck/spatial/VENDOR.md`). The Apache 2.0
attribution above covers the *format and step grammar* the Nexus
corpus reuses; the corpus content itself is original and is licensed
under Apache 2.0 by the Nexus contributors.

If the openCypher Implementers Group ever opens a spatial track,
the Nexus-authored corpus is eligible for upstream contribution.

## Other crates and libraries

Per-crate notices for runtime + dev dependencies are tracked by
`cargo` and visible via `cargo licenses`. This file is reserved for
attributions that fall outside the dependency graph (vendored test
corpora, copy-pasted reference implementations, etc.).
