## 1. LMDB catalog
- [ ] 1.1 Decide hook surface: `heed` wrapper vs `mdb_env_set_userctx`
- [ ] 1.2 Wire `EncryptedPageStream` into the catalog read/write path
- [ ] 1.3 Verify metadata-Raft snapshot install still works

## 2. Record stores
- [ ] 2.1 Wire encryption into the node store mmap write-back
- [ ] 2.2 Wire encryption into the relationship store
- [ ] 2.3 Wire encryption into the property store + string store
- [ ] 2.4 Verify Windows mmap path (per-OS quirks)

## 3. Page cache
- [ ] 3.1 Decrypt on read, encrypt on eviction
- [ ] 3.2 Verify cache-coherence under concurrent eviction
- [ ] 3.3 Update page-cache stats to count crypto hits/misses

## 4. Startup invariants
- [ ] 4.1 Reject mixed-mode databases with a clear error
- [ ] 4.2 Validate the EaR magic on every page during recovery scan

## 5. Tests
- [ ] 5.1 Round-trip: write encrypted, read back identical
- [ ] 5.2 Crash + recovery: encrypted db survives kill -9
- [ ] 5.3 Bench: throughput overhead vs un-encrypted baseline (target ≤ 15%)

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass
