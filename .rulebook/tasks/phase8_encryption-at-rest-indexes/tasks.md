## 1. B-tree index
- [ ] 1.1 Wire EncryptedPageStream into the leaf+internal page write path
- [ ] 1.2 Verify range scans still hit performance target

## 2. Full-text (Tantivy)
- [ ] 2.1 Inventory Tantivy segment files; assign FileId::FullTextIndex per file
- [ ] 2.2 Wire the SegmentReader / SegmentWriter through the page stream
- [ ] 2.3 Verify async writer crash-recovery still works

## 3. KNN (HNSW)
- [ ] 3.1 Wire encryption into the hnsw_rs serialised file layout
- [ ] 3.2 Verify HNSW index reload after restart

## 4. R-tree
- [ ] 4.1 Wire encryption into the packed-Hilbert R-tree file
- [ ] 4.2 Verify spatial query path performance

## 5. Tests
- [ ] 5.1 Round-trip per index type
- [ ] 5.2 Cross-restart consistency
- [ ] 5.3 Wrong-key test surfaces ERR_BAD_KEY cleanly

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass
