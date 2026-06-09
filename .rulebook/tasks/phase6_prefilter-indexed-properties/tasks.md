## 1. Investigation
- [ ] 1.1 Confirm the O(props x labels) loop + per-prop LMDB read in `maintain_indexed_properties` (crud.rs:1293) and the no-early-exit behavior
- [ ] 1.2 Identify the source of truth for indexed (label_id,key_id) pairs to mirror cheaply (and keep in sync with CREATE/DROP INDEX + #11 startup rebuild)

## 2. Implementation
- [ ] 2.1 Maintain an in-memory indexed-pairs set; pre-filter the property map so only indexed (label,key) pairs are resolved/inserted, with an early exit when the node has no indexed label
- [ ] 2.2 Keep the set updated on CREATE INDEX / DROP INDEX and the startup rebuild

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation (CHANGELOG / GH #21)
- [ ] 3.2 Write tests: indexed properties are still maintained correctly; a node with no indexed label does zero index work; CREATE/DROP INDEX updates the prefilter set
- [ ] 3.3 Run tests and confirm they pass
