## 1. Investigation
- [x] 1.1 Reproduce: non-ASCII body (CREATE (:T {v:'versão'})) -> panic at parser/tokens.rs:151 (dropped connection)
- [x] 1.2 Locate the decode site: lexer `consume_char` advanced `pos` by 1 byte, not `ch.len_utf8()` -> mid-UTF-8 slice panic
- [x] 1.3 Determine connection drop cause: the lexer PANIC (not a JSON error) tore down the request

## 2. Implementation
- [x] 2.1 Decode end-to-end: `consume_char` + keyword/whitespace scan loops advance by `ch.len_utf8()` (UTF-8-correct)
- [x] 2.2 Non-ASCII in string literals, property values, WHERE comparisons, and $params round-trips losslessly
- [x] 2.3 No panic on UTF-8; malformed JSON body is still rejected by the extractor without dropping the connection

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the fix (CHANGELOG Unreleased / GH #6)
- [x] 3.2 Write tests: accented Latin, CJK, Cyrillic, emoji via CREATE/MATCH/WHERE/params round-trip exactly (nexus-core cypher_non_ascii_test.rs + nexus-server handler test)
- [x] 3.3 Run tests and confirm they pass (executor 3 + server 1; nexus-core lib serial 2354 green; clippy/fmt clean)
