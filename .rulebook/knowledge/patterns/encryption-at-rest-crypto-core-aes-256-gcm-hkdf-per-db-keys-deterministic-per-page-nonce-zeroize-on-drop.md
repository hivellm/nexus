# Encryption-at-rest crypto core: AES-256-GCM + HKDF per-DB keys + deterministic per-page nonce + zeroize-on-drop

**Category**: security
**Tags**: security, encryption-at-rest, aes-gcm, hkdf, rust, compliance

## Description

For encryption at rest in a database engine, ship the crypto core as a self-contained module before wiring it into the storage layer. Five pieces: (1) `KeyProvider` trait with `EnvKeyProvider` + `FileKeyProvider` defaults plus a seam for KMS adapters; (2) HKDF-SHA-256 per-database key derivation with a rotation `epoch` parameter and a domain-separation tag mixed into the `info` string; (3) AES-256-GCM page cipher with deterministic 96-bit nonce derived from `(file_id, page_offset, generation)` — the generation counter is non-negotiable because nonce reuse breaks AES-GCM catastrophically; (4) `EncryptedPageStream` wrapper that owns the generation counter, lays down a 16-byte plaintext page header (magic + file_id + generation), and binds the header into the AEAD as AAD so adversarial header swaps are detected at decrypt; (5) every secret wrapped in `zeroize::Zeroizing` so it gets wiped on drop. Carve storage-layer wiring (catalog, record stores, WAL, indexes) into per-module follow-ups — each has its own invariants.

## Example

// Crypto-core composition:
let provider = EnvKeyProvider::from_default_env()?;       // master key
let master = MasterKey::new(*provider.master_key()?);
let db_key = derive_database_key(&master, "default", 0)?; // HKDF
let cipher = PageCipher::new(&db_key);
let stream = EncryptedPageStream::new(cipher);
let page  = stream.encrypt(FileId::NodeStore, offset, plaintext)?;  // gen++
let pt    = stream.decrypt(offset, page.as_slice())?;

// Nonce derivation contract (96 bits = 12 bytes):
//   bytes [0..2]  = file_id      (16 bits)
//   bytes [2..8]  = page_offset  (low 48 bits)
//   bytes [8..12] = generation   (32 bits)
// The generation counter MUST bump on every overwrite of the same page.

## When to Use

Any database engine that needs SOC2 / FedRAMP / HIPAA / PCI-DSS compliance. The crypto core in isolation is testable, cryptanalytically defensible, and forward-compatible with KMS adapters and online rotation.

## When NOT to Use

If the deployment relies entirely on disk-level encryption (LUKS, BitLocker, AWS EBS encryption) and the threat model accepts memory-resident master key exposure, EaR-in-engine adds complexity without protecting a different threat. Engine-side EaR shines specifically when key material must NOT touch the disk via the OS keyring.
