## 1. Implementation

Consecutive `CREATE` clauses in one query do not share variable bindings: in
`CREATE (a:A), (b:B)` followed by `CREATE (a)-[:T]->(b)`, the second clause
mints fresh anonymous nodes instead of reusing `a`/`b` (5 nodes where the TCK
expects 3). Root-cause area: engine write-path clause loop
(`engine/write_exec/`). Most TCK fixtures build graphs with the multi-clause
CREATE idiom, so this silently corrupts the setup of many scenarios across
categories (verified while implementing pattern comprehensions: collapsing to a
single CREATE clause produces the exact expected result).

- [ ] 1.1 later CREATE clauses reuse variables bound by earlier CREATE/MATCH clauses in the same query (nodes and relationships); unbound names still create fresh entities
- [ ] 1.2 mixed sequences work: MATCH-then-CREATE reusing matched nodes, CREATE-then-CREATE chains, and RETURN of variables bound in any earlier clause

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
