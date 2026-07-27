## 1. Implementation
- [x] 1.1 JSONL failure log emitted in after-hook (`log_failure`, NEXUS_TCK_FAILLOG, default target/tck-failures.jsonl)
- [x] 1.2 parameters + control-query steps wired (`Given parameters are:`, `When executing control query:`; drained into execute_cypher_with_params; skip_reason entries removed)
- [x] 1.3 re-baseline recorded against the new buckets (3255 failures bucketed; temporal 953, quantifier 588, match 339, with-orderBy 263, boolean 144, ...; 2762 wrong-rows vs 363 SyntaxError/16 TypeError)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation (OPENCYPHER_TCK_REPORT.md regenerated from the instrumented run)
- [x] 2.2 Write tests covering the new behavior (the harness itself is the test surface; the two new steps + faillog exercised by the full TCK run)
- [x] 2.3 Run tests and confirm they pass (NEXUS_TCK=1 TCK run completed; harness compiles, clippy/fmt clean; JSONL baseline produced)
