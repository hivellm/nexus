## 1. Implementation
- [ ] 1.1 Enforce inline target-node labels in required expand (multi-hop chains: `(a:A)-->(b:X)-->(c:X)` must filter `c` by `:X`; audit planner pattern-lowering for dropped label info)
- [ ] 1.2 Enforce target labels in optional expand (label-rejected candidate = no match → NULL-padded row per LEFT-OUTER semantics, reusing the all-rejected padding path)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
