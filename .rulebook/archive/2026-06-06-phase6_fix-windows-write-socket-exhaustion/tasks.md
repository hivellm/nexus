## 1. Investigation
- [x] 1.1 Reproduce socket exhaustion: per-request connection churn (each request opens a new TCP connection -> TIME_WAIT pileup)
- [x] 1.2 Source of churn: nexus-protocol RestClient built a fresh reqwest::Client per post/get/stream (own empty pool each call)
- [x] 1.3 Inspect server + client transports: Axum/hyper keep-alive is default; the Rust SDK transport already reused its client; nexus-protocol did not

## 2. Implementation
- [x] 2.1 nexus-protocol RestClient builds one reqwest::Client (bounded idle pool + TCP keep-alive) and reuses it across all requests
- [x] 2.2 Rust SDK HTTP transport gains explicit pool_max_idle_per_host + pool_idle_timeout + tcp_keepalive (parity / Windows robustness)
- [x] 2.3 Connection reuse confirmed by test (5 sequential POSTs share one connection) -> client-side batch+retry+fallback workaround no longer required

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the fix (CHANGELOG Unreleased)
- [x] 3.2 Write tests: connection-counting keep-alive server asserts <=2 connections for 5 sequential requests (reuses_connection_across_sequential_requests)
- [x] 3.3 Run tests and confirm they pass (nexus-protocol 84 lib tests + reuse test green; SDK lib clippy clean + transport tests pass)
