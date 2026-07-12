## 1. PackStream v2
- [ ] 1.1 PackStream encoder/decoder (all core types + Node/Relationship/UnboundRelationship/Path structures)
- [ ] 1.2 Temporal + spatial structure encodings (Date, Time, DateTime, Duration, Point2D/3D)

## 2. Connection protocol
- [ ] 2.1 TCP listener (default 7687, configurable) + handshake/version negotiation (Bolt v5.x)
- [ ] 2.2 HELLO/LOGON with auth bridge to existing auth layer; GOODBYE; RESET
- [ ] 2.3 RUN/PULL/DISCARD with result streaming (records in batches, has_more)
- [ ] 2.4 BEGIN/COMMIT/ROLLBACK mapped to engine transactions
- [ ] 2.5 ROUTE stub (single-instance routing table) so `neo4j://` URIs work

## 3. Query execution
- [ ] 3.1 Route all Bolt queries through `Engine::execute_cypher_with_params` (unified entry point; prerequisite: phase1 unification complete)
- [ ] 3.2 Map ResultSet → PackStream records with Bolt metadata (fields, t_first, t_last, qid)

## 4. Driver validation (gate)
- [ ] 4.1 neo4j-python-driver: connect, parameterized read+write, tx, streaming — green
- [ ] 4.2 neo4j-javascript-driver: same battery — green
- [ ] 4.3 Neo4j Browser connects and renders a graph

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 5.1 Update or create documentation covering the implementation (deployment guide, docker-compose port, README)
- [ ] 5.2 Write tests covering the new behavior (PackStream roundtrip, protocol state machine, driver integration suite)
- [ ] 5.3 Run tests and confirm they pass
