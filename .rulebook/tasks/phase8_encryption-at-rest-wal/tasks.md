## 1. WAL append
- [ ] 1.1 Encrypt frame payload via EncryptedPageStream before append
- [ ] 1.2 Maintain CRC32C over plaintext for end-to-end integrity
- [ ] 1.3 Bind frame metadata (lsn, term) into AEAD as AAD

## 2. WAL replay
- [ ] 2.1 Decrypt frame payload during replay
- [ ] 2.2 Treat AEAD failure on trailing frame as truncated (parity with CRC mismatch)
- [ ] 2.3 Surface ERR_WAL_AEAD for non-trailing AEAD failures

## 3. Tests
- [ ] 3.1 Crash + recovery test: encrypted WAL survives kill -9
- [ ] 3.2 Tampered-frame test: bit-flip in mid-WAL → clean truncate point reported
- [ ] 3.3 Wrong-key test: replay with wrong master surfaces ERR_BAD_KEY

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation
- [ ] 4.2 Write tests covering the new behavior
- [ ] 4.3 Run tests and confirm they pass
