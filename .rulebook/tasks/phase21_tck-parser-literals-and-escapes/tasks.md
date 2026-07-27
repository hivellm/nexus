## 1. Implementation
- [x] 1.1 extended numeric literal forms — `parse_numeric_literal` (primary.rs) rewritten to accept radix prefixes (`0x1F`/`0X`, `0o17`/`0O` → `finish_radix_int`), scientific notation (`1e10`, `1.5E-3`), `_` digit-group separators (`1_000`, `0x_FF`, stripped before parsing), and leading-dot floats (`.5`, dispatched from both `parse_simple_expression`/`parse_primary_expression` when a digit follows the dot). The fractional `.` is consumed ONLY when a digit follows it, so `1..3` ranges / slice bounds are never misparsed as a float (regression test).
- [x] 1.2 extended string escapes — `parse_string_literal` (primary.rs) now decodes `\b` (backspace), `\f` (form feed), `\0` (null), and `\uXXXX` (four hex digits → `char::from_u32`, errors on a malformed escape), alongside the existing `\n`/`\t`/`\r`/`\\`.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation (doc comments on `parse_numeric_literal`/`finish_radix_int`/the `\u` arm; rationale + anchors here)
- [x] 2.2 Write tests covering the new behavior (tests/cypher/parser_literals_test.rs — 8 tests: scientific, hex/octal, underscores, leading-dot, range-not-misparsed, \uXXXX, control-char escapes, existing escapes)
- [x] 2.3 Run tests and confirm they pass (cypher 450/0; clippy/fmt clean; full workspace gate running)
