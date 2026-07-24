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
| Schema prep DDL | **done** |
| Nexus bulk loader (`ldbc-load`) | **done** — loads and verifies SF0.1 |
| Neo4j baseline loader (`neo4j-load`) | **done** — same graph into Neo4j |
| Short reads IS1–IS7 | **ported**; differential-validated against Neo4j |
| Complex reads IC1–IC14 | not started |
| Updates INS1–INS8 | not started |
| Bench driver | not started |
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

## Schema preparation

Apply the index DDL to a **fresh** database before loading, so the loader
populates the indexes incrementally instead of paying for a rebuild afterwards:

```bash
./schema/create-schema.sh --url http://localhost:15474
```

`schema/indexes.cypher` declares 15 property indexes (8 id lookups, 3 name
lookups, 4 creationDate ranges). Label indexes are not declared: Nexus keeps a
RoaringBitmap per label automatically, so `MATCH (n:Person)` is already indexed.
Every statement is `IF NOT EXISTS`, so re-running is a no-op.

Bash only, matching `scripts/bench/`, which is also bash-only — the alternative
is two implementations of the same probe logic that must be kept in lockstep.

### One database per server process

**The harness deliberately has no `--database` flag.** Nexus currently ignores
the `database` field on `POST /cypher` and `PUT /session/database` reports
success without switching, so every query lands in the same store regardless of
what was requested (filed as `phase0_fix-cypher-database-routing`). Rather than
ship a flag that silently does nothing, the harness assumes one database per
server process: start a server on a dedicated `NEXUS_DATA_DIR` and point
`--url` at it.

### How coverage is verified

Nexus has no `SHOW INDEXES` (filed as `phase7_opencypher-gap-closure` item 4.6),
so the script cannot read the index set back. It instead uses the planner's
`Nexus.Performance.UnindexedPropertyAccess` notification, which fires when a
label+property predicate falls back to a label scan — its absence is positive
evidence the index is registered and used.

That signal only fires when the label has rows to scan, so on an empty database
the check would pass vacuously (confirmed: probing a database with *no* indexes
at all reported every index present). The script therefore creates one throwaway
node per label, probes, then deletes them and asserts they are gone — a leaked
probe row would corrupt the loader's post-load cardinality verification.

## Loading

```bash
cd loader && cargo build --release        # stable Rust; the harness is not in the Nexus workspace
./target/release/ldbc-load \
  --dataset ~/.cache/ldbc-snb/sf0.1/social_network-sf0.1-CsvCompositeMergeForeign-LongDateFormatter \
  --url http://localhost:15474
```

The database must be **empty** and its indexes created first — the loader
verifies absolute counts, so pre-existing rows are reported as a mismatch.

| Flag | Purpose |
|---|---|
| `--dry-run` | Parse every file and resolve every foreign key without writing. Needs no server; proves the dataset is internally consistent in ~2 s. |
| `--verify-only` | Re-check an already-loaded database without reloading it. |
| `--strict-readback` | Make the per-type traversal read-back fatal (see the caveat below). |
| `--batch-rows` / `--batch-bytes` | Request sizing. Both ceilings apply — a batch of long-`content` Posts is orders of magnitude bigger than a batch of Tags, and the server's body limit is 16 MiB. |

### How the load works

1. **Nodes**, static labels first, recording `(label, LDBC id) → internal id`
   from the ids `/ingest` returns in input order. The map is keyed per label
   because LDBC ids are only unique *within* a label — SF0.1 has a Place 0, an
   Organisation 0, a Tag 0 and a Forum 0.
2. **Merge-foreign relationships**, by re-reading the node files for their FK
   columns. A second streaming pass rather than buffering: an edge needs both
   endpoints to exist, and holding ~600 k (SF0.1) or ~6 M (SF1) pending edges
   in memory is worse than a few seconds of I/O.
3. **The ten edge files**, one edge per row in the recorded direction. `KNOWS`
   is stored **once per friendship** (LDBC's convention) and is NOT mirrored:
   the reference queries traverse it undirected (`-[:KNOWS]-`), which Nexus
   serves in both directions off the store adjacency index. Mirroring would
   double every friendship under that match. Both engines load it identically.

SF0.1 totals: **327 588 nodes, 1 477 965 relationships** — the 576 896 edge-file
rows plus 901 069 synthesized merge-foreign edges.

Both loaders write the identical logical graph: same labels (including the
`:Message` superlabel on Posts and Comments), same merge-foreign edges, same
single-direction `KNOWS`, dates as epoch-millisecond integers. That is what
makes the differential validation a like-for-like comparison.

```bash
# Load the Neo4j baseline (fast: ~30 s; resolves edges by id index, no id map)
cd loader && cargo build --release
./target/release/neo4j-load --dataset <dir> --url http://localhost:17474
```

### Temporal encoding — epoch milliseconds, deliberately

`creationDate`, `joinDate` and `birthday` are stored as **integers**, exactly as
`LongDateFormatter` writes them, not converted to ISO-8601 strings:

- Nexus has no native temporal property type — `datetime()` returns a string.
- The benchmark's own substitution parameters express dates as epoch millis
  (`interactive_2_param.txt`: `maxDate = 1354060800000`), so the queries compare
  like with like instead of round-tripping through a string format.
- Integer comparisons are served by the property B-tree's range seek; a string
  encoding would only order correctly by lexicographic accident.

The Neo4j baseline must load them the same way or the comparison is not
like-for-like.

### Measured on SF0.1 (release build, localhost, 2026-07-24)

| Phase | Rate |
|---|---|
| Nodes via `/ingest` | ~58 000 nodes/s |
| Relationships via `/ingest` | ~3 100 rel/s |
| Whole SF0.1 load | ~480 s |

The node figure is the first empirical confirmation of the `/ingest` rewrite
(`phase0_fix-ingest-bulk-path` §3.2, which deferred its re-measurement here):
469 nodes/s before, ~58 000 nodes/s now. Relationship creation is now the
bottleneck by a factor of ~19.

### Verification is split by trustworthiness

| Check | Status |
|---|---|
| Per-label node counts | exact and stable — **fatal** on mismatch |
| Total relationship count (engine write counter, `/stats`) | exact — **fatal** on mismatch |
| Per-type `MATCH ()-[r:T]->()` read-back | advisory by default, **fatal under `--strict-readback`** |

The per-type read-back was once non-deterministic — three consecutive runs of a
read-only database returned different, always-short counts — because the engine
mistook any node carrying a property named `type` (LDBC's `Organisation.type`,
`Place.type`) for a relationship, and the row-deduplication key then collapsed
unrelated rows. That was **`phase7_opencypher-gap-closure` item 4.8** and is now
**fixed**; `ldbc-load --strict-readback` passes every per-type count on SF0.1.
The advisory-by-default split is kept as a guard: it stays green under normal
runs and only a future regression of that class would trip it, and `--strict-readback`
makes such a regression a hard failure in CI.

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
separate edge file. The loader must synthesize these — 13 foreign-key columns
carrying 8 of the schema's 15 relationship types (the other 7 come from the
edge files above) — and they are easy to miss when counting rows:

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
- `person_knows_person` is undirected and stored **once per pair**. The loaders
  keep it single-direction (as the CSV records it) and rely on the undirected
  `-[:KNOWS]-` match to traverse both ways — the reference behaviour.

## Query status

Ported queries live under `queries/`. `scripts/validate-short-reads.py` runs
each short read against BOTH engines on the same loaded SF0.1 graph, sampling
real ids from the database, and marks a query ✅ only when Nexus and Neo4j
return the same result set on every sampled id.

```bash
python scripts/validate-short-reads.py \
    --nexus http://localhost:15474 --neo4j http://localhost:17474
```

| Query | Status | Note |
|---|---|---|
| IS1 | ✅ | matches Neo4j on every sampled id |
| IS2 | ⛔ BLOCKED | a variable-length path (`REPLY_OF*0..`) expanded from a `WITH`-carried variable does not bind its target on Nexus (`phase7_opencypher-gap-closure` 4.11). The fresh-MATCH form works, so the port is faithful. |
| IS3 | ✅ | matches Neo4j (friends via undirected `-[:KNOWS]-`) |
| IS4 | ✅ | matches Neo4j |
| IS5 | ✅ | matches Neo4j |
| IS6 | ✅ | matches Neo4j |
| IS7 | ✅ | matches Neo4j (OPTIONAL MATCH + CASE) |
| IC1–IC14 | — | not ported yet |
| INS1–INS8 | — | not ported yet |

Fixing the query-correctness phase surfaced and closed four engine bugs
(`phase7_opencypher-gap-closure` items 4.8–4.10 plus the relationship-traversal
adjacency-index repoint): a `type` property misread as a relationship, N²/first-
row multi-pattern writes, and — the big one — incoming/undirected traversal that
returned nothing on graphs with more than ~10 000 relationships. IS2 remains
blocked on a fifth (4.11, the `WITH`-carried variable-length expand).

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
