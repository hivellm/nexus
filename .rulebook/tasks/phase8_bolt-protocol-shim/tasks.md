## 1. Wire protocol
- [ ] 1.1 Implement PackStream codec (encode + decode) in `crates/nexus-server/src/transport/bolt/packstream.rs`
- [ ] 1.2 Implement Bolt v5 handshake (version negotiation)
- [ ] 1.3 Implement Bolt frame parser: `HELLO`, `LOGON`, `BEGIN`, `RUN`, `PULL`, `DISCARD`, `COMMIT`, `ROLLBACK`, `RESET`, `LOGOFF`, `GOODBYE`

## 2. Executor mapping
- [ ] 2.1 Map `RUN` to `Engine::execute` with parameter binding
- [ ] 2.2 Map `BEGIN` / `COMMIT` / `ROLLBACK` to transaction layer
- [ ] 2.3 Map `PULL` / `DISCARD` to result-stream chunking
- [ ] 2.4 Map `RESET` to connection-state cleanup
- [ ] 2.5 Map `HELLO` + `LOGON` to API-key + JWT auth

## 3. Auth
- [ ] 3.1 Support `principal/credentials` basic-auth
- [ ] 3.2 Support `bearer` JWT auth
- [ ] 3.3 Map auth failures to Bolt `FAILURE` with proper error codes

## 4. Server integration
- [ ] 4.1 Add `--enable-bolt` flag + `NEXUS_BOLT_PORT` env var (default 7687)
- [ ] 4.2 Wire Bolt listener alongside HTTP + RPC listeners
- [ ] 4.3 Apply same rate limiting + admission queue + audit log as other transports

## 5. Tests
- [ ] 5.1 Wire-level codec unit tests (encode → decode round-trip every Bolt message type)
- [ ] 5.2 Integration test against official `neo4j-driver` Python (point read, traversal, transaction)
- [ ] 5.3 Integration test against official `neo4j-driver-java`
- [ ] 5.4 Integration test against `cypher-shell` CLI
- [ ] 5.5 Auth failure tests (bad creds, expired JWT)

## 6. Documentation
- [ ] 6.1 Add Bolt section to `docs/specs/api-protocols.md`
- [ ] 6.2 Add "Connecting with Neo4j drivers" guide under `docs/integrations/`
- [ ] 6.3 Document explicit scope gaps (multi-DB routing, cluster routing)
- [ ] 6.4 Add to README transports table

## 7. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 7.1 Update or create documentation covering the implementation
- [ ] 7.2 Write tests covering the new behavior
- [ ] 7.3 Run tests and confirm they pass
