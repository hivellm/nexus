# LDBC SNB Interactive — Nexus benchmark harness

## Scope statement

Read this before quoting any number produced here.

- **Workload**: LDBC Social Network Benchmark **Interactive** (v1) — 7 short
  reads (IS1–IS7), 14 complex reads (IC1–IC14), 8 updates (INS1–INS8). The
  **Business Intelligence** workload is explicitly out of scope.
- **Unaudited**: these are LDBC-*compatible* results, **not** an LDBC-*audited*
  benchmark. An audited run requires the official LDBC Java driver, which is
  blocked on Nexus having no JVM SDK. Results from this harness must never be
  published as "LDBC audited" or compared against audited vendor results as
  though they were like-for-like.
- **In-repo REST harness**: the driver here talks to Nexus over its HTTP
  `/cypher` endpoint, not the official driver. HTTP framing and JSON
  serialization are inside the measured path, so absolute latencies carry
  per-request protocol overhead that an embedded or Bolt client would not pay.
  The Neo4j baseline is driven the same way, so the *comparison* stays fair even
  though the absolute numbers are not the engine's floor.
- **Scale factors**: SF0.1 for correctness smoke runs, SF1 for reported
  numbers. SF10+ is manual, on dedicated hardware.

Engine gaps discovered here are **filed**, never worked around: a query Nexus
cannot express or answers differently gets a repro logged against
`phase7_opencypher-gap-closure` and is marked BLOCKED in the query table below.
Simplifying a benchmark query to make it pass is prohibited — a fast wrong
answer is worthless.

## Status

| Component | State |
|---|---|
| Dataset fetch + checksum pinning | **done** |
| Schema prep DDL | not started |
| Bulk loader | not started |
| Short reads IS1–IS7 | not started |
| Complex reads IC1–IC14 | not started |
| Updates INS1–INS8 | not started |
| Bench driver | not started |
| Neo4j baseline mode | not started |
| SF1 report | not started |

Tracked as `.rulebook/tasks/phase7_ldbc-snb-benchmark`.

## Quick start

```bash
# POSIX (Linux, macOS, Git Bash)
./fetch-dataset.sh                 # SF0.1 — 22 MiB of downloads
./fetch-dataset.sh --scale 1       # SF1  — 278 MiB of downloads
./fetch-dataset.sh --scale all --verify-only --no-extract   # re-check cache
```

```powershell
# Windows PowerShell 7+
.\fetch-dataset.ps1
.\fetch-dataset.ps1 -Scale 1
.\fetch-dataset.ps1 -Scale all -VerifyOnly -NoExtract
```

Both read the same `dataset-manifest.tsv`, so URLs and checksums are pinned in
exactly one place.

### Prerequisites

- `curl` (POSIX script only — the PowerShell script uses `Invoke-WebRequest`)
- A zstd decompressor: either the `zstd` CLI alongside `tar`, or Python with
  `pip install zstandard`. The scripts probe for the CLI first and fall back to
  Python, which is the common case on Windows.
- Free disk, measured on 2026-07-19 (archives are kept alongside the extracted
  CSV so re-extraction needs no re-download):

  | Scale | Archives | Extracted | Total |
  |---|---:|---:|---:|
  | SF0.1 | 22 MiB | 110 MiB | ~135 MiB |
  | SF1 | 278 MiB | 1.2 GiB | ~1.5 GiB |

### Cache location

Datasets are cached **outside the git tree** and are never committed. The cache
root is resolved in this order:

1. `--cache DIR` / `-Cache DIR`
2. `$LDBC_SNB_CACHE_DIR`
3. `~/.cache/ldbc-snb`

Layout under the cache root:

```
archives/                                   # verified .tar.zst downloads
sf0.1/
  social_network-sf0.1-CsvCompositeMergeForeign-LongDateFormatter/
    static/    organisation, place, tag, tagclass
    dynamic/   person, forum, post, comment + edge files
  substitution_parameters-sf0.1/            # interactive_N_param.txt
  social_network-sf0.1-numpart-1/           # updateStream_*_person.csv, _forum.csv
sf1/
  ...
```

`.gitignore` in this directory mirrors that layout as a safety net for anyone
who points `--cache` at the repository.

## Dataset

Pre-generated LDBC artifacts from `datasets.ldbcouncil.org`, serializer
**CsvCompositeMergeForeign** with **LongDateFormatter** (dates as epoch
milliseconds). This serializer produces the fewest files — 18 CSVs — and is the
layout the reference implementations assume.

| Scale | Dataset | Parameters | Update streams |
|---|---|---|---|
| 0.1 | 16 MiB | 199 KiB | 6.0 MiB |
| 1 | 202 MiB | 502 KiB | 76 MiB |

URLs, SHA-256 checksums and byte sizes live in `dataset-manifest.tsv`. LDBC
publishes no checksum file for Interactive v1, so the pinned hashes were
computed locally from the downloaded archives on 2026-07-19. **Cached archives
are always re-hashed** — neither script has a flag that skips verification.

Re-pinning is deliberate: download, confirm the contents are what you expect,
then update `dataset-manifest.tsv` in the same commit.

### SF0.1 expected cardinalities

Reference counts for the loader's post-load verification (record counts, header
row excluded):

| Node file | Records | Edge file | Records |
|---|---:|---|---:|
| `person` | 1 528 | `person_knows_person` | 14 073 |
| `forum` | 13 750 | `forum_hasMember_person` | 123 268 |
| `post` | 135 701 | `forum_hasTag_tag` | 47 697 |
| `comment` | 151 043 | `post_hasTag_tag` | 51 118 |
| `place` | 1 460 | `comment_hasTag_tag` | 191 303 |
| `organisation` | 7 955 | `person_hasInterest_tag` | 35 475 |
| `tag` | 16 080 | `person_likes_post` | 47 215 |
| `tagclass` | 71 | `person_likes_comment` | 62 225 |
| | | `person_studyAt_organisation` | 1 209 |
| | | `person_workAt_organisation` | 3 313 |
| **Total nodes** | **327 588** | **Total edge-file rows** | **576 896** |

### Merge-foreign edges

The `MergeForeign` serializer folds every **single-cardinality** relationship
into the owning node's CSV as a foreign-key column instead of emitting a
separate edge file. The loader must synthesize these — they account for 9 of
the schema's relationship types and are easy to miss when counting rows:

| Source file | FK column | Relationship |
|---|---|---|
| `place` | `isPartOf` | `IS_PART_OF` → Place |
| `organisation` | `place` | `IS_LOCATED_IN` → Place |
| `tag` | `hasType` | `HAS_TYPE` → TagClass |
| `tagclass` | `isSubclassOf` | `IS_SUBCLASS_OF` → TagClass |
| `person` | `place` | `IS_LOCATED_IN` → Place |
| `forum` | `moderator` | `HAS_MODERATOR` → Person |
| `post` | `creator` | `HAS_CREATOR` → Person |
| `post` | `Forum.id` | `CONTAINER_OF` ← Forum |
| `post` | `place` | `IS_LOCATED_IN` → Place |
| `comment` | `creator` | `HAS_CREATOR` → Person |
| `comment` | `place` | `IS_LOCATED_IN` → Place |
| `comment` | `replyOfPost` / `replyOfComment` | `REPLY_OF` → Post / Comment |

`isPartOf` is empty for continents and `isSubclassOf` is empty for the root
TagClass; `replyOfPost` and `replyOfComment` are mutually exclusive. Rows with
an empty FK produce no edge rather than an edge to a null target.

Other layout notes that bite loaders:

- Fields are **pipe (`|`) separated**, not comma separated.
- `person.language` and `person.email` are `;`-separated multi-values inside a
  single field.
- Dates are epoch **milliseconds** (`LongDateFormatter`), including
  `person.birthday`, which is a date-only value expressed as a UTC midnight
  timestamp.
- `person_knows_person` is undirected and stored **once per pair**; both
  directions must be materialized for the queries to traverse correctly.

## Query status

Filled in as IS/IC/INS queries are ported and validated against Neo4j at SF0.1.
A query is only marked ✅ once Nexus and Neo4j return the same result set.

| Query | Status | Note |
|---|---|---|
| IS1–IS7 | — | not ported yet |
| IC1–IC14 | — | not ported yet |
| INS1–INS8 | — | not ported yet |

## Neo4j baseline

The baseline reuses the pinned Neo4j container in `scripts/bench/`:

```bash
scripts/bench/neo4j-up.sh      # bolt localhost:17687, http localhost:17474
scripts/bench/neo4j-down.sh    # tears down and drops the data volume
```

Ports are deliberately off the Neo4j defaults so a local Neo4j Desktop install
does not collide with the bench container.

## References

- LDBC SNB specification: <https://arxiv.org/pdf/2001.02299>
- Interactive v1 reference implementations:
  <https://github.com/ldbc/ldbc_snb_interactive_v1_impls>
- Dataset repository: <https://ldbcouncil.org/benchmarks/snb/datasets/>
