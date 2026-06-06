# Proposal: phase6_fix-cypher-nonascii-body

Source: GitHub issue #6 (https://github.com/hivellm/nexus/issues/6)

## Why
`POST /cypher` rejects non-ASCII bytes in the request body with
`invalid unicode code point` and DROPS the connection. Any query
containing UTF-8 text (accented words, non-Latin scripts, emoji) fails
hard. Downstream callers must currently strip non-ASCII from the payload
as a lossy workaround (e.g. "versão" -> "verso"), corrupting data on
write and breaking round-trips. UTF-8 in property values and string
literals must be fully supported — Cypher/JSON are UTF-8 by spec.

## What Changes
- Find where the `/cypher` body is decoded/parsed and why a valid UTF-8
  (or specific multi-byte) sequence triggers `invalid unicode code
  point`. Likely culprits: a byte-wise / ASCII-only scan in the Cypher
  lexer/parser, a `char`-cast from a single byte, a manual UTF-8 decode,
  or a body reader that assumes ASCII. The error string `invalid unicode
  code point` pinpoints the offending decode site.
- Decode the body as UTF-8 end-to-end and let the lexer/parser operate on
  `char`s / proper UTF-8, so non-ASCII in string literals, property
  values, parameters, and identifiers (where Cypher allows) round-trips
  losslessly.
- The connection MUST NOT be dropped on a decode error: malformed input
  returns a structured 4xx JSON error, never a transport teardown.
- Add tests with accented Latin, non-Latin scripts (e.g. Cyrillic/CJK),
  and emoji through `CREATE`/`MATCH`/parameters, asserting exact
  round-trip (e.g. `versão` stays `versão`).

## Impact
- Affected specs: api-protocols / cypher-subset (lexing)
- Affected code: `crates/nexus-server/src/api/` (body decode/handler),
  `crates/nexus-core/src/executor/parser/` (lexer/parser char handling)
- Breaking change: NO (fixes a hard failure; response format unchanged)
- User benefit: full UTF-8 support; no lossy client-side stripping; no
  connection drops on international text
