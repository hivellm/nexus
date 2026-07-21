# Reading chain pointers without decoding the rel_id+1 encoding

**Category**: code
**Tags**: storage, record-store, chain-walk, off-by-one, merge

## Description

RecordStore relationship chain pointers (first_rel_ptr, next_src_ptr, next_dst_ptr) are stored as rel_id + 1 so that 0 can serve as the end-of-chain sentinel (see record_store_ops create_relationship). Any chain walker MUST decode with ptr - 1 before read_rel(), like executor/operators/path.rs does (verify_rel_id = rel_ptr.saturating_sub(1)). The engine-side find_relationship_between walk read read_rel(rel_ptr) undecoded for its whole life — the 'authoritative' fallback silently returned None or off-by-one rel ids whenever the exact-edge index missed, letting edge-MERGE create duplicates. Fixed in phase6_relwalk-warn-at-fallback (#20).

## Example

let rel_id = rel_ptr - 1; // pointers are rel_id + 1; 0 = end of chain
let rel_record = self.storage.read_rel(rel_id)?;

## When to Use

Whenever following node→relationship chain pointers from a NodeRecord or RelationshipRecord.
