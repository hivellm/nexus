## 1. Implementation
- [x] 1.1 Reproduce: `POST /cypher` with `{"query":"RETURN 1 AS x","parameters":null}` -> 422
- [x] 1.2 Make `CypherRequest.params` accept explicit `null` (and absent / `parameters` alias / `params`) as an empty map (deserialize_with null->default)
- [x] 1.3 Verify `parameters:null`, `{}`, omitted all 200; a real map still binds; `parameters` and `params` both accepted

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation (CHANGELOG Fixed / GH #7)
- [x] 2.2 Write tests covering the new behavior (serde: null/absent/{}/map and the parameters alias all deserialize to the right param map)
- [x] 2.3 Run tests and confirm they pass
