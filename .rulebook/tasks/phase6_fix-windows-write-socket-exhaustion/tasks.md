## 1. Investigation
- [ ] 1.1 Reproduce socket exhaustion on Windows under sustained per-request writes (TIME_WAIT / ephemeral port drain)
- [ ] 1.2 Determine the source of per-request connection churn (server keep-alive config vs client opening a new connection per request)
- [ ] 1.3 Inspect server HTTP keep-alive (Axum/hyper) and the protocol/SDK client transport connection reuse

## 2. Implementation
- [ ] 2.1 Enable/honor HTTP keep-alive end-to-end so repeated writes reuse a pooled connection
- [ ] 2.2 Ensure the first-party client/SDK transport reuses a keep-alive connection pool (no new socket per request)
- [ ] 2.3 Provide/confirm a batched or pipelined write path so the client-side batch+retry+fallback workaround is unnecessary

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the fix (transport / connection management)
- [ ] 3.2 Write tests: sustained write load reuses connections (no per-request socket churn); validate on Windows
- [ ] 3.3 Run tests and confirm they pass
