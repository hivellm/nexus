## 1. Investigation
- [ ] 1.1 Reproduce: POST /cypher with non-ASCII body (e.g. CREATE (:T {v:'versão'})) -> "invalid unicode code point" + dropped connection
- [ ] 1.2 Locate the decode site emitting "invalid unicode code point" (body reader vs lexer/parser byte-cast)
- [ ] 1.3 Determine why the connection is torn down rather than returning a JSON error

## 2. Implementation
- [ ] 2.1 Decode the /cypher body as UTF-8 end-to-end; lexer/parser operate on chars, not raw bytes
- [ ] 2.2 Support non-ASCII in string literals, property values, and parameters (round-trip lossless)
- [ ] 2.3 On malformed input, return a structured 4xx JSON error without dropping the connection

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the fix (api-protocols UTF-8 support)
- [ ] 3.2 Write tests: accented Latin, non-Latin script, emoji via CREATE/MATCH/params round-trip exactly
- [ ] 3.3 Run tests and confirm they pass
