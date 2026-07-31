## 1. Implementation
- [ ] 1.1 Canonicalise temporal values on the SET write path in `engine/write_exec/properties.rs` before they are persisted, covering `SET n.p = <expr>`, `SET n = {map}` (whole-entity replace) and `SET n += {map}`, plus the MERGE `ON CREATE` / `ON MATCH` variants
- [ ] 1.2 Consolidate canonicalisation into a single property-write choke point every write path converges on (replacing the manual boundary points enumerated in temporal_value.rs:604-632) — or, if they must stay separate, document that reason in place of the consolidation
- [ ] 1.3 Round-trip test reading through the property store (not only through the projection boundary, which masks the stale tag): a value written by CREATE and the same value written by SET have identical on-disk representation, and a value stored in the legacy raw tagged form still reads back correctly

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
