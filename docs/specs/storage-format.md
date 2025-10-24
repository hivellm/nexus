# Storage Format Specification

This document defines the exact binary format for Nexus record stores.

## File Layout

### Directory Structure

```
data/
├── catalog.mdb          # LMDB catalog (labels/types/keys)
├── catalog.mdb-lock     # LMDB lock file
├── nodes.store          # Node records
├── rels.store           # Relationship records
├── props.store          # Property records
├── strings.store        # String/blob dictionary
├── wal.log              # Write-ahead log
├── checkpoints/         # Checkpoint snapshots
│   ├── epoch_1000.ckpt
│   └── epoch_2000.ckpt
└── indexes/             # Index files
    ├── label_0.bitmap   # Label bitmap for label 0
    ├── label_1.bitmap
    ├── hnsw_0.bin       # HNSW index for label 0
    └── fulltext_0.idx   # Tantivy index for label 0 (V1)
```

## Record Store Formats

### nodes.store

**Fixed-size records**: 32 bytes per node

```
Offset | Size | Field         | Description
-------|------|---------------|----------------------------------
0      | 8    | label_bits    | Bitmap of label IDs (max 64 labels)
8      | 8    | first_rel_ptr | Pointer to first relationship (offset in rels.store)
16     | 8    | prop_ptr      | Pointer to first property (offset in props.store)
24     | 4    | flags         | Status flags (see below)
28     | 4    | reserved      | Reserved for future use (padding)
```

**Node ID Calculation**:
```
node_id = record_offset / 32
record_offset = node_id * 32
```

**Flags Field** (32 bits):
```
Bit    | Meaning
-------|----------------------------------------------------------
0      | Deleted (soft delete, GC later)
1      | Locked (transaction in progress)
2-7    | Reserved
8-31   | Version/epoch (for MVCC, 24 bits = 16M versions)
```

**label_bits Encoding**:
```
Each bit represents presence of a label:
- Bit 0 set: has label_id 0
- Bit 1 set: has label_id 1
- etc.

Example: 0x0000000000000005 = labels 0 and 2
```

**Special Values**:
```
first_rel_ptr = 0xFFFFFFFFFFFFFFFF  → no relationships
prop_ptr      = 0xFFFFFFFFFFFFFFFF  → no properties
```

### rels.store

**Fixed-size records**: 48 bytes per relationship

```
Offset | Size | Field          | Description
-------|------|----------------|----------------------------------
0      | 8    | src_id         | Source node ID
8      | 8    | dst_id         | Destination node ID
16     | 4    | type_id        | Relationship type ID
20     | 4    | padding        | Padding for alignment
24     | 8    | next_src_ptr   | Next outgoing rel from src (offset in rels.store)
32     | 8    | next_dst_ptr   | Next incoming rel to dst (offset in rels.store)
40     | 8    | prop_ptr       | Pointer to first property (offset in props.store)
```

**Relationship ID Calculation**:
```
rel_id = record_offset / 48
record_offset = rel_id * 48
```

**Linked List Traversal**:
```
# Get all outgoing relationships from node N:
rel_ptr = nodes[N].first_rel_ptr
while rel_ptr != 0xFFFFFFFFFFFFFFFF:
    rel = rels[rel_ptr]
    if rel.src_id == N:
        yield rel
        rel_ptr = rel.next_src_ptr
    else:  # rel.dst_id == N
        rel_ptr = rel.next_dst_ptr

# Get all incoming relationships to node N:
rel_ptr = nodes[N].first_rel_ptr
while rel_ptr != 0xFFFFFFFFFFFFFFFF:
    rel = rels[rel_ptr]
    if rel.dst_id == N:
        yield rel
        rel_ptr = rel.next_dst_ptr
    else:  # rel.src_id == N
        rel_ptr = rel.next_src_ptr
```

### props.store

**Variable-size records**: Property chains

```
PropertyRecord (header + value):

Offset | Size     | Field     | Description
-------|----------|-----------|----------------------------------
0      | 4        | key_id    | Property key ID
4      | 1        | type      | Value type (see below)
5      | 3        | padding   | Alignment
8      | varies   | value     | Value bytes (size depends on type)
8+size | 8        | next_ptr  | Pointer to next property (offset)
```

**Value Types** (1 byte):
```
Type | Value | Size  | Encoding
-----|-------|-------|----------------------------------------
NULL | 0x00  | 0     | (no value bytes)
BOOL | 0x01  | 1     | 0x00=false, 0x01=true
I64  | 0x02  | 8     | Little-endian signed 64-bit integer
F64  | 0x03  | 8     | Little-endian IEEE 754 double
STR  | 0x04  | 8     | Offset in strings.store (u64)
BLOB | 0x05  | 8     | Offset in strings.store (u64)
```

**Total Record Size**:
```
NULL: 4 + 1 + 3 + 0 + 8 = 16 bytes
BOOL: 4 + 1 + 3 + 1 + 8 = 17 bytes (padded to 24)
I64:  4 + 1 + 3 + 8 + 8 = 24 bytes
F64:  4 + 1 + 3 + 8 + 8 = 24 bytes
STR:  4 + 1 + 3 + 8 + 8 = 24 bytes (reference)
BLOB: 4 + 1 + 3 + 8 + 8 = 24 bytes (reference)

Alignment: All records padded to 8-byte boundaries
```

### strings.store

**Variable-size records**: String/blob dictionary

```
StringRecord:

Offset | Size     | Field      | Description
-------|----------|------------|----------------------------------
0      | varint   | length     | Length in bytes (LEB128 encoded)
N      | length   | data       | UTF-8 string or raw bytes
N+len  | 4        | crc32      | CRC32 checksum of data
N+len+4| padding  | padding    | Pad to 8-byte boundary
```

**Varint Encoding** (LEB128):
```
Length < 128:     1 byte
Length < 16384:   2 bytes
Length < 2097152: 3 bytes
etc.

Example:
127    → 0x7F
128    → 0x80 0x01
16383  → 0xFF 0x7F
16384  → 0x80 0x80 0x01
```

**Reference Counting** (for deduplication, V1):
```
Optional optimization: Store reference count before length
┌───────────┬────────┬────────┬────────┐
│ ref_count │ length │  data  │  crc32 │
│ (varint)  │(varint)│        │        │
└───────────┴────────┴────────┴────────┘

When ref_count reaches 0, entry can be garbage collected.
```

## Page Structure

All files (except LMDB catalog) are organized into pages.

### Page Header (16 bytes)

```
Offset | Size | Field      | Description
-------|------|------------|----------------------------------
0      | 8    | page_id    | Logical page number
8      | 4    | checksum   | xxHash3 of page data
12     | 2    | flags      | Page flags (dirty, pinned, etc.)
14     | 2    | reserved   | Reserved
```

### Page Body (8176 bytes for 8KB pages)

```
Total page size: 8192 bytes (8KB)
Header: 16 bytes
Body: 8176 bytes (usable data)
```

**Records Per Page**:
```
Node records: 8176 / 32 = 255 nodes per page
Rel records:  8176 / 48 = 170 relationships per page
Prop records: Variable (depends on property sizes)
```

## Catalog Format (LMDB)

### Tables

#### label_name_to_id
```
Key: String (label name, e.g., "Person")
Value: u32 (label ID, little-endian)
```

#### label_id_to_name
```
Key: u32 (label ID, big-endian for sorting)
Value: String (label name)
```

#### type_name_to_id
```
Key: String (type name, e.g., "KNOWS")
Value: u32 (type ID, little-endian)
```

#### type_id_to_name
```
Key: u32 (type ID, big-endian)
Value: String (type name)
```

#### key_name_to_id
```
Key: String (key name, e.g., "name", "age")
Value: u32 (key ID, little-endian)
```

#### key_id_to_name
```
Key: u32 (key ID, big-endian)
Value: String (key name)
```

#### statistics
```
Key: String (stat name, e.g., "node_count:0" for label 0)
Value: u64 (stat value, little-endian)

Stat types:
- "node_count:<label_id>"
- "rel_count:<type_id>"
- "avg_degree:<label_id>:<type_id>"
- "ndv:<label_id>:<key_id>"  (number distinct values, for optimizer)
```

#### metadata
```
Key: String (metadata key)
Value: Bytes (metadata value, format depends on key)

Examples:
- "version" → "0.1.0" (semver string)
- "created_at" → Unix timestamp (u64)
- "page_size" → 8192 (u32)
- "epoch" → Current epoch (u64)
```

## Index Formats

### Label Bitmap (RoaringBitmap)

```
File: indexes/label_<label_id>.bitmap

Format: Roaring bitmap serialized format
- Cookie: 4 bytes (version)
- Container count: 4 bytes
- Containers: Variable size
  - Each container: type (array/bitmap/run) + data

Reference: https://github.com/RoaringBitmap/RoaringFormatSpec
```

### HNSW Vector Index

```
File: indexes/hnsw_<label_id>.bin

Custom format:
┌────────────┬────────────┬────────────┬────────────┐
│  Header    │  Graph     │  Vectors   │  Mapping   │
└────────────┴────────────┴────────────┴────────────┘

Header (64 bytes):
- Magic: 0x4E455855534B4E4E ("NEXUSKNN")
- Version: u32
- Dimension: u32
- M: u32 (max connections per layer)
- ef_construction: u32
- Distance metric: u8 (0=cosine, 1=euclidean)
- Node count: u64
- Reserved: 27 bytes

Graph (variable):
- HNSW graph structure (adjacency lists per layer)

Vectors (dimension * node_count * 4 bytes):
- f32 vectors packed (little-endian)

Mapping (node_count * 16 bytes):
- node_id (u64) → embedding_idx (u64) pairs
- Sorted by node_id for binary search
```

## Append-Only Guarantees

All store files are append-only until compaction:

```
Write Pattern:
1. Append new record to end of file
2. Update WAL with record offset
3. Update pointers in existing records (e.g., linked lists)
4. Commit WAL entry
5. Flush dirty pages

Compaction (periodic):
1. Freeze writes
2. Scan all records
3. Write live records to new file (reordered for locality)
4. Update pointers
5. Atomically swap files
6. Unfreeze writes
```

## Endianness

**All multi-byte integers**: Little-endian

Rationale: x86/x64 (most deployment targets) is little-endian; avoid byte swaps.

## Alignment

**All records**: Aligned to 8-byte boundaries

Rationale: Faster access on modern CPUs; allows direct casting in Rust with `bytemuck`.

## Checksums

### xxHash3

Used for page checksums (fast, good quality):
```
checksum = xxh3_64(page_data[16..])
```

### CRC32

Used for strings.store (compact, hardware-accelerated):
```
crc32 = crc32c(string_data)
```

## File Growth Strategy

```
Initial allocation: 1MB (128 pages for 8KB pages)
Growth factor: 2x when full
Max file size: 1TB (practical limit)

Example progression:
1MB → 2MB → 4MB → 8MB → 16MB → ... → 1TB
```

## Compatibility

### Version Evolution

```
Version 0.1.0 (MVP):
- Fixed-size node/rel records
- Varint props/strings
- No compression

Version 0.2.0 (V1, planned):
- Optional property compression (LZ4)
- Delta encoding for sorted IDs
- Column-oriented property storage (optional)

Version 0.3.0 (V2, planned):
- Distributed format (shard metadata)
- Cross-shard relationship pointers
```

### Migration

```
On version upgrade:
1. Read old format
2. Write new format to temp files
3. Validate checksums
4. Atomically replace old files
5. Update catalog metadata version
```

## Performance Characteristics

```
Operation          | Complexity | Notes
-------------------|------------|----------------------------------
Node read by ID    | O(1)       | Direct offset: node_id * 32
Rel read by ID     | O(1)       | Direct offset: rel_id * 48
Expand neighbors   | O(degree)  | Linked list traversal
Property lookup    | O(props)   | Traverse property chain
String lookup      | O(1)       | Direct offset in strings.store

Sequential scan (nodes): ~500 MB/sec (memmap)
Random read (nodes):     ~100K ops/sec (page cache)
Write (append):          ~50K ops/sec (WAL + cache)
```

## Example Binary Layout

### Sample Node Record

```
Node ID: 42
Labels: [0, 2] (Person, Employee)
Properties: {name: "Alice", age: 30}
Relationships: 2 outgoing

Binary (hex):
Offset 0x0540 (node_id 42):
05 00 00 00 00 00 00 00  ← label_bits (0x05 = labels 0,2)
A0 12 00 00 00 00 00 00  ← first_rel_ptr (offset 0x12A0)
10 34 00 00 00 00 00 00  ← prop_ptr (offset 0x3410)
00 00 00 00              ← flags (0 = active)
00 00 00 00              ← reserved
```

### Sample Relationship Record

```
Rel ID: 100
Src: 42, Dst: 99
Type: 1 (KNOWS)
Properties: {since: 2020}

Binary (hex):
Offset 0x12A0 (rel_id 100):
2A 00 00 00 00 00 00 00  ← src_id (42)
63 00 00 00 00 00 00 00  ← dst_id (99)
01 00 00 00              ← type_id (1)
00 00 00 00              ← padding
D0 12 00 00 00 00 00 00  ← next_src_ptr (0x12D0)
FF FF FF FF FF FF FF FF  ← next_dst_ptr (none)
50 34 00 00 00 00 00 00  ← prop_ptr (0x3450)
```

## Security Considerations

### Data At Rest

```
MVP: No encryption (plaintext files)

V1 (optional): AES-256-GCM encryption
- Encrypt pages individually
- Store key in external key management system
- Performance overhead: ~10-20%
```

### Data Integrity

```
Checksums on all records:
- Detect corruption
- Fail fast on read errors
- WAL replay validates checksums

Backup strategy:
- Copy files while quiesced
- Verify checksums on restore
```

## Debugging Tools

### Hexdump Example

```bash
# View first node record
hexdump -C data/nodes.store | head -n 3

00000000  05 00 00 00 00 00 00 00  a0 12 00 00 00 00 00 00  |................|
00000010  10 34 00 00 00 00 00 00  00 00 00 00 00 00 00 00  |.4..............|
00000020  ...
```

### CLI Inspection (future)

```bash
nexus-cli inspect nodes.store --id 42
# Output:
# Node ID: 42
# Labels: [Person, Employee]
# Properties: {name: "Alice", age: 30}
# Relationships: 2 outgoing, 1 incoming
```

## Further Reading

- RoaringBitmap Spec: https://github.com/RoaringBitmap/RoaringFormatSpec
- LMDB Internals: http://www.lmdb.tech/doc/
- Neo4j Record Format: https://neo4j.com/docs/ (conceptual reference)

