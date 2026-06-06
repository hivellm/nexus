## 1. Investigation
- [ ] 1.1 Reproduce the prop_ptr corruption with a populated persistent volume across two boots
- [ ] 1.2 Identify the write path that emits a node prop_ptr pointing at a relationship id
- [ ] 1.3 Determine whether corruption is write-time or WAL-replay-ordering
- [ ] 1.4 Check whether the recovery reset write is flushed to the catalog/record store before shutdown

## 2. Implementation
- [ ] 2.1 Fix the corruption source (write-time ptr-type guard or replay ordering)
- [ ] 2.2 Make recovery durable so subsequent boots are clean (one-shot)
- [ ] 2.3 Fix the serializer race so in-flight recovery never yields invalid JSON
- [ ] 2.4 Ensure recovered nodes return full property map on RETURN n

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the fix (storage-format / wal-mvcc)
- [ ] 3.2 Write tests: corrupted prop_ptr recovers durably; second boot is clean; whole-node serialization is race-free
- [ ] 3.3 Run tests and confirm they pass
