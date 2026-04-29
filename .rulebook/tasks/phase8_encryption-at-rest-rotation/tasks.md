## 1. Two-key window
- [ ] 1.1 Extend EncryptedPageStream to hold an optional secondary key
- [ ] 1.2 Read-path: try primary key, fall back to secondary on ERR_BAD_KEY
- [ ] 1.3 Write-path: always use the primary

## 2. Background runner
- [ ] 2.1 Walk every page lowest-offset first
- [ ] 2.2 Re-encrypt: decrypt under secondary, encrypt under primary, bump generation
- [ ] 2.3 Throttle to a configurable byte budget per second

## 3. Coordinator
- [ ] 3.1 CLI: `nexus admin rotate-key --database <name>`
- [ ] 3.2 Progress reporting via Prometheus counters
- [ ] 3.3 Resume from checkpoint after a server restart

## 4. Tests
- [ ] 4.1 Rotate while serving traffic — no downtime, no read errors
- [ ] 4.2 Crash mid-rotation — resume from checkpoint
- [ ] 4.3 Verify the old key is dropped after completion

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 5.1 Update or create documentation covering the implementation
- [ ] 5.2 Write tests covering the new behavior
- [ ] 5.3 Run tests and confirm they pass
