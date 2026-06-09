## 1. Investigation
- [ ] 1.1 Confirm the warn at mod.rs:3343 fires only after the while-loop completes; decide the in-loop threshold + fallback-entry log points

## 2. Implementation
- [ ] 2.1 Log at chain-walk fallback entry (fast-path miss) and fire the hop-threshold warning during the loop, not after; keep it O(1) logging

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation (CHANGELOG / GH #20)
- [ ] 3.2 Write tests: a high-degree fallback emits the warning at the threshold (capture via a tracing test subscriber or a counter), not only at completion
- [ ] 3.3 Run tests and confirm they pass
