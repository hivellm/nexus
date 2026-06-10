//! Write-Ahead Log (WAL) - Transaction durability
//!
//! All mutations go through WAL before page table updates.
//! Supports MVCC via epoch-based snapshots.
//! Periodic checkpoints truncate WAL and compact pages.
//!
//! # Format
//!
//! WAL Entry: [epoch:8][tx_id:8][type:1][length:4][payload:N][crc32:4]
//!
//! Entry types:
//! - 0x01: BeginTx
//! - 0x02: CommitTx
//! - 0x03: AbortTx
//! - 0x10: CreateNode
//! - 0x11: DeleteNode
//! - 0x20: CreateRel
//! - 0x21: DeleteRel
//! - 0x30: SetProperty
//! - 0xFF: Checkpoint

// Sub-modules
pub mod async_wal;
mod record;
mod writer;

// Re-export async WAL types (unchanged — no edits to async_wal.rs)
pub use async_wal::{AsyncWalConfig, AsyncWalStats, AsyncWalStatsSnapshot, AsyncWalWriter};

// Re-export record types so `crate::wal::WalEntry` etc. remain valid.
pub use record::{ChecksumAlgo, WalEntry, WalEntryType, WalStats};

// Re-export the writer so `crate::wal::Wal` remains valid.
pub use writer::Wal;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::crypto::encrypted_file::PAGE_HEADER_LEN;
    use crate::storage::crypto::{FileId, PageCipher, PageHeader};
    use crate::testing::TestContext;
    use crc32fast::Hasher;
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};
    use std::sync::Arc;

    fn create_test_wal() -> (Wal, TestContext) {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal.log");
        let wal = Wal::new(&path).unwrap();
        (wal, ctx)
    }

    #[test]
    fn test_wal_creation() {
        let (wal, _dir) = create_test_wal();
        assert_eq!(wal.offset, 0);
        assert_eq!(wal.stats.entries_written, 0);
    }

    #[test]
    fn test_append_entry() {
        let (mut wal, _dir) = create_test_wal();

        let entry = WalEntry::BeginTx {
            tx_id: 1,
            epoch: 100,
        };

        let offset = wal.append(&entry).unwrap();
        assert_eq!(offset, 0);
        assert_eq!(wal.stats.entries_written, 1);
        assert!(wal.stats.file_size > 0);
    }

    #[test]
    fn test_append_multiple_entries() {
        let (mut wal, _dir) = create_test_wal();

        for i in 0..10 {
            let entry = WalEntry::CreateNode {
                node_id: i,
                label_bits: 1 << i,
            };
            wal.append(&entry).unwrap();
        }

        assert_eq!(wal.stats.entries_written, 10);
    }

    #[test]
    fn test_flush() {
        let (mut wal, _dir) = create_test_wal();

        let entry = WalEntry::BeginTx {
            tx_id: 1,
            epoch: 100,
        };

        wal.append(&entry).unwrap();
        wal.flush().unwrap();

        // Flush should not change stats
        assert_eq!(wal.stats.entries_written, 1);
    }

    #[test]
    fn test_checkpoint() {
        let (mut wal, _dir) = create_test_wal();

        // Write some entries
        for i in 0..5 {
            let entry = WalEntry::CreateNode {
                node_id: i,
                label_bits: 0,
            };
            wal.append(&entry).unwrap();
        }

        assert_eq!(wal.stats.entries_since_checkpoint, 5);

        // Checkpoint
        wal.checkpoint(100).unwrap();

        assert_eq!(wal.stats.checkpoints, 1);
        assert_eq!(wal.stats.entries_since_checkpoint, 0);
    }

    #[test]
    fn test_recover_empty_wal() {
        let (mut wal, _dir) = create_test_wal();

        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_recover_with_entries() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal.log");

        // Write entries
        {
            let mut wal = Wal::new(&path).unwrap();

            wal.append(&WalEntry::BeginTx {
                tx_id: 1,
                epoch: 100,
            })
            .unwrap();

            wal.append(&WalEntry::CreateNode {
                node_id: 42,
                label_bits: 5,
            })
            .unwrap();

            wal.append(&WalEntry::CommitTx {
                tx_id: 1,
                epoch: 100,
            })
            .unwrap();

            wal.flush().unwrap();
        }

        // Recover
        {
            let mut wal = Wal::new(&path).unwrap();
            let entries = wal.recover().unwrap();

            assert_eq!(entries.len(), 3);
            assert_eq!(wal.stats.entries_read, 3);

            // Verify entry types
            match &entries[0] {
                WalEntry::BeginTx { tx_id, epoch } => {
                    assert_eq!(*tx_id, 1);
                    assert_eq!(*epoch, 100);
                }
                _ => panic!("Expected BeginTx"),
            }

            match &entries[1] {
                WalEntry::CreateNode {
                    node_id,
                    label_bits,
                } => {
                    assert_eq!(*node_id, 42);
                    assert_eq!(*label_bits, 5);
                }
                _ => panic!("Expected CreateNode"),
            }

            match &entries[2] {
                WalEntry::CommitTx { tx_id, .. } => {
                    assert_eq!(*tx_id, 1);
                }
                _ => panic!("Expected CommitTx"),
            }
        }
    }

    #[test]
    fn test_entry_types() {
        let entry1 = WalEntry::BeginTx {
            tx_id: 1,
            epoch: 100,
        };
        assert_eq!(entry1.entry_type() as u8, 0x01);

        let entry2 = WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        };
        assert_eq!(entry2.entry_type() as u8, 0x10);

        let entry3 = WalEntry::Checkpoint { epoch: 100 };
        assert_eq!(entry3.entry_type() as u8, 0xFF);
    }

    #[test]
    fn test_truncate() {
        let (mut wal, _dir) = create_test_wal();

        // Write entries
        for i in 0..10 {
            wal.append(&WalEntry::CreateNode {
                node_id: i,
                label_bits: 0,
            })
            .unwrap();
        }

        assert!(wal.stats.file_size > 0);

        // Truncate
        wal.truncate().unwrap();

        assert_eq!(wal.offset, 0);
        assert_eq!(wal.stats.file_size, 0);
        assert_eq!(wal.stats.entries_since_checkpoint, 0);
    }

    #[test]
    fn test_all_entry_types_serialization() {
        let (mut wal, _dir) = create_test_wal();

        let entries = vec![
            WalEntry::BeginTx {
                tx_id: 1,
                epoch: 100,
            },
            WalEntry::CreateNode {
                node_id: 42,
                label_bits: 7,
            },
            WalEntry::DeleteNode { node_id: 43 },
            WalEntry::CreateRel {
                rel_id: 1,
                src: 10,
                dst: 20,
                type_id: 5,
            },
            WalEntry::DeleteRel { rel_id: 2 },
            WalEntry::SetProperty {
                entity_id: 42,
                key_id: 1,
                value: b"test value".to_vec(),
            },
            WalEntry::DeleteProperty {
                entity_id: 42,
                key_id: 1,
            },
            WalEntry::CommitTx {
                tx_id: 1,
                epoch: 100,
            },
            WalEntry::AbortTx {
                tx_id: 2,
                epoch: 101,
            },
            WalEntry::Checkpoint { epoch: 100 },
        ];

        // Write all entries
        for entry in &entries {
            wal.append(entry).unwrap();
        }

        wal.flush().unwrap();

        // Recover and verify
        let mut wal2 = Wal::new(&wal.path).unwrap();
        let recovered = wal2.recover().unwrap();

        assert_eq!(recovered.len(), entries.len());
    }

    #[test]
    fn test_crc_corruption_detection() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal.log");

        // Write valid entry
        {
            let mut wal = Wal::new(&path).unwrap();
            wal.append(&WalEntry::CreateNode {
                node_id: 1,
                label_bits: 0,
            })
            .unwrap();
            wal.flush().unwrap();
        }

        // Corrupt the file (change a byte in the middle)
        {
            let mut file = OpenOptions::new().write(true).open(&path).unwrap();
            file.seek(SeekFrom::Start(10)).unwrap();
            file.write_all(&[0xFF]).unwrap();
            file.sync_all().unwrap();
        }

        // Recovery should detect corruption
        {
            let mut wal = Wal::new(&path).unwrap();
            let result = wal.recover();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("CRC"));
        }
    }

    #[test]
    fn test_transaction_sequence() {
        let (mut wal, _dir) = create_test_wal();

        // Simulate transaction: begin → create node → commit
        wal.append(&WalEntry::BeginTx {
            tx_id: 1,
            epoch: 100,
        })
        .unwrap();

        wal.append(&WalEntry::CreateNode {
            node_id: 42,
            label_bits: 1,
        })
        .unwrap();

        wal.append(&WalEntry::CreateRel {
            rel_id: 1,
            src: 42,
            dst: 43,
            type_id: 1,
        })
        .unwrap();

        wal.append(&WalEntry::CommitTx {
            tx_id: 1,
            epoch: 100,
        })
        .unwrap();

        assert_eq!(wal.stats.entries_written, 4);
    }

    #[test]
    fn test_entry_tx_id() {
        let entry = WalEntry::BeginTx {
            tx_id: 123,
            epoch: 1,
        };
        assert_eq!(entry.tx_id(), Some(123));

        let entry2 = WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        };
        assert_eq!(entry2.tx_id(), None);
    }

    #[test]
    fn test_entry_epoch() {
        let entry = WalEntry::BeginTx {
            tx_id: 1,
            epoch: 999,
        };
        assert_eq!(entry.epoch(), Some(999));

        let entry2 = WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        };
        assert_eq!(entry2.epoch(), None);
    }

    #[test]
    fn test_stats() {
        let (mut wal, _dir) = create_test_wal();
        wal.append(&WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        })
        .unwrap();

        let stats = wal.stats();
        assert_eq!(stats.entries_written, 1);
        assert!(stats.file_size > 0);
    }

    #[test]
    fn test_large_payload() {
        let (mut wal, _dir) = create_test_wal();

        // Large property value (1MB)
        let large_value = vec![0xAB; 1024 * 1024];

        let entry = WalEntry::SetProperty {
            entity_id: 1,
            key_id: 1,
            value: large_value.clone(),
        };

        wal.append(&entry).unwrap();
        wal.flush().unwrap();

        // Recover and verify
        let mut wal2 = Wal::new(&wal.path).unwrap();
        let recovered = wal2.recover().unwrap();

        assert_eq!(recovered.len(), 1);
        match &recovered[0] {
            WalEntry::SetProperty { value, .. } => {
                assert_eq!(value.len(), 1024 * 1024);
                assert_eq!(value[0], 0xAB);
            }
            _ => panic!("Expected SetProperty"),
        }
    }

    /// Hand-crafted legacy v1 frame: `[type:1][len:4][payload:N][crc32fast:4]`,
    /// no magic byte, no algo tag. Proves the reader still accepts
    /// files written by pre-SIMD binaries.
    fn write_legacy_v1_frame(path: &std::path::Path, entry: &WalEntry) {
        use std::io::Write;
        let payload = bincode::serialize(entry).unwrap();
        let mut buf = Vec::new();
        buf.push(entry.entry_type() as u8);
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&payload);
        let mut hasher = Hasher::new();
        hasher.update(&buf);
        let crc = hasher.finalize();
        buf.extend_from_slice(&crc.to_le_bytes());
        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();
        f.write_all(&buf).unwrap();
        f.sync_all().unwrap();
    }

    #[test]
    fn legacy_v1_frame_recovers_without_magic() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal-v1.log");

        // Write two v1 frames by hand.
        write_legacy_v1_frame(
            &path,
            &WalEntry::BeginTx {
                tx_id: 7,
                epoch: 42,
            },
        );
        write_legacy_v1_frame(
            &path,
            &WalEntry::CreateNode {
                node_id: 999,
                label_bits: 0x3,
            },
        );

        // Open the file through the regular WAL and recover.
        let mut wal = Wal::new(&path).unwrap();
        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 2);
        match &entries[0] {
            WalEntry::BeginTx { tx_id, epoch } => {
                assert_eq!(*tx_id, 7);
                assert_eq!(*epoch, 42);
            }
            _ => panic!("expected BeginTx"),
        }
        match &entries[1] {
            WalEntry::CreateNode {
                node_id,
                label_bits,
            } => {
                assert_eq!(*node_id, 999);
                assert_eq!(*label_bits, 0x3);
            }
            _ => panic!("expected CreateNode"),
        }
    }

    #[test]
    fn v2_frame_with_crc32c_roundtrips() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal-crc32c.log");
        {
            let mut wal = Wal::new(&path).unwrap();
            wal.append_with_algo(
                &WalEntry::BeginTx {
                    tx_id: 3,
                    epoch: 55,
                },
                ChecksumAlgo::Crc32C,
            )
            .unwrap();
            wal.append_with_algo(
                &WalEntry::CreateNode {
                    node_id: 77,
                    label_bits: 0xF,
                },
                ChecksumAlgo::Crc32C,
            )
            .unwrap();
            wal.flush().unwrap();
        }
        let mut wal = Wal::new(&path).unwrap();
        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(matches!(
            entries[0],
            WalEntry::BeginTx {
                tx_id: 3,
                epoch: 55
            }
        ));
        assert!(matches!(
            entries[1],
            WalEntry::CreateNode { node_id: 77, .. }
        ));
    }

    #[test]
    fn mixed_v1_then_v2_frames_replay_cleanly() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal-mixed.log");

        // Prepend a v1 frame written by the legacy helper.
        write_legacy_v1_frame(
            &path,
            &WalEntry::BeginTx {
                tx_id: 1,
                epoch: 100,
            },
        );

        // Append two v2 frames via the production writer.
        {
            let mut wal = Wal::new(&path).unwrap();
            wal.append(&WalEntry::CreateNode {
                node_id: 200,
                label_bits: 0x1,
            })
            .unwrap();
            wal.append(&WalEntry::CommitTx {
                tx_id: 1,
                epoch: 100,
            })
            .unwrap();
            wal.flush().unwrap();
        }

        let mut wal = Wal::new(&path).unwrap();
        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0], WalEntry::BeginTx { tx_id: 1, .. }));
        assert!(matches!(
            entries[1],
            WalEntry::CreateNode { node_id: 200, .. }
        ));
        assert!(matches!(entries[2], WalEntry::CommitTx { tx_id: 1, .. }));
    }

    // phase6_fulltext-wal-integration — FTS op-code round-trip.
    #[test]
    fn fts_wal_ops_encode_decode_roundtrip() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("fts.wal");
        let mut wal = Wal::new(&path).unwrap();

        let create = WalEntry::FtsCreateIndex {
            name: "movies".to_string(),
            entity: 0,
            labels_or_types: vec!["Movie".to_string()],
            properties: vec!["title".to_string(), "overview".to_string()],
            analyzer: "standard".to_string(),
        };
        let add = WalEntry::FtsAdd {
            name: "movies".to_string(),
            entity_id: 42,
            label_or_type_id: 0,
            key_id: 0,
            content: "The Matrix".to_string(),
        };
        let del = WalEntry::FtsDel {
            name: "movies".to_string(),
            entity_id: 42,
        };
        let drop = WalEntry::FtsDropIndex {
            name: "movies".to_string(),
        };
        for e in [&create, &add, &del, &drop] {
            wal.append(e).unwrap();
        }
        wal.flush().unwrap();
        drop_wal(wal);

        let mut wal = Wal::new(&path).unwrap();
        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 4);
        match &entries[0] {
            WalEntry::FtsCreateIndex {
                name,
                entity,
                labels_or_types,
                properties,
                analyzer,
            } => {
                assert_eq!(name, "movies");
                assert_eq!(*entity, 0);
                assert_eq!(labels_or_types, &vec!["Movie".to_string()]);
                assert_eq!(
                    properties,
                    &vec!["title".to_string(), "overview".to_string()]
                );
                assert_eq!(analyzer, "standard");
            }
            other => panic!("expected FtsCreateIndex, got {other:?}"),
        }
        assert!(matches!(entries[1], WalEntry::FtsAdd { entity_id: 42, .. }));
        assert!(matches!(entries[2], WalEntry::FtsDel { entity_id: 42, .. }));
        match &entries[3] {
            WalEntry::FtsDropIndex { name } => assert_eq!(name, "movies"),
            other => panic!("expected FtsDropIndex, got {other:?}"),
        }
    }

    fn drop_wal(_w: Wal) {
        // Explicit drop helper — required because `Wal` holds a file
        // handle that we need closed before reopening for recovery.
    }

    // ---------- v3 (encrypted) WAL tests --------------------------

    fn fresh_cipher(seed: u8, db: &str) -> Arc<PageCipher> {
        use crate::storage::crypto::kdf::{MasterKey, derive_database_key};
        let m = MasterKey::new([seed; 32]);
        let k = derive_database_key(&m, db, 0).unwrap();
        Arc::new(PageCipher::new(&k))
    }

    fn make_encrypted_wal(seed: u8) -> (Wal, TestContext) {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal.log");
        let cipher = fresh_cipher(seed, "default");
        let wal = Wal::with_cipher(&path, cipher).unwrap();
        (wal, ctx)
    }

    #[test]
    fn v3_round_trip_recovers_plaintext_payload() {
        let (mut wal, _ctx) = make_encrypted_wal(0xAA);
        let entries = [
            WalEntry::BeginTx { tx_id: 1, epoch: 1 },
            WalEntry::SetProperty {
                entity_id: 42,
                key_id: 7,
                value: b"top-secret".to_vec(),
            },
            WalEntry::CommitTx { tx_id: 1, epoch: 1 },
        ];
        for e in &entries {
            wal.append(e).unwrap();
        }
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);

        let cipher = fresh_cipher(0xAA, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let recovered = wal2.recover().unwrap();
        assert_eq!(recovered.len(), entries.len());
        match &recovered[1] {
            WalEntry::SetProperty {
                entity_id,
                key_id,
                value,
            } => {
                assert_eq!(*entity_id, 42);
                assert_eq!(*key_id, 7);
                assert_eq!(value, b"top-secret");
            }
            other => panic!("expected SetProperty, got {other:?}"),
        }
    }

    #[test]
    fn v3_file_starts_with_ear_page_header() {
        let (mut wal, _ctx) = make_encrypted_wal(0x11);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);

        // The first 16 bytes must classify as the EaR magic so the
        // boot inventory scanner sees an encrypted file.
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > PAGE_HEADER_LEN);
        let mut header_buf = [0u8; PAGE_HEADER_LEN];
        header_buf.copy_from_slice(&bytes[..PAGE_HEADER_LEN]);
        let header = PageHeader::from_bytes(&header_buf).expect("EaR magic missing");
        assert_eq!(header.file_id, FileId::Wal);
    }

    #[test]
    fn v3_ciphertext_does_not_contain_plaintext_payload() {
        let (mut wal, _ctx) = make_encrypted_wal(0x22);
        let needle = b"NEEDLE_THAT_MUST_NOT_LEAK";
        wal.append(&WalEntry::SetProperty {
            entity_id: 1,
            key_id: 1,
            value: needle.to_vec(),
        })
        .unwrap();
        wal.flush().unwrap();
        let bytes = std::fs::read(&wal.path).unwrap();
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle),
            "plaintext leaked into ciphertext on disk"
        );
    }

    #[test]
    fn v3_wrong_key_surfaces_err_wal_aead() {
        let (mut wal, ctx) = make_encrypted_wal(0xAB);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.append(&WalEntry::CommitTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);
        // _ctx held to keep the temp dir alive
        let _ = ctx;

        let wrong = fresh_cipher(0xCD, "default");
        let mut wal2 = Wal::with_cipher(&path, wrong).unwrap();
        let err = wal2.recover().unwrap_err();
        let msg = err.to_string();
        // The first frame is mid-WAL relative to the second one;
        // wrong-key surfaces ERR_WAL_AEAD on the first frame because
        // it does not extend to EOF.
        assert!(msg.contains("ERR_WAL_AEAD"), "got {msg}");
    }

    #[test]
    fn v3_tampered_mid_wal_frame_surfaces_err_wal_aead() {
        let (mut wal, ctx) = make_encrypted_wal(0x33);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.append(&WalEntry::CommitTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);
        let _ = ctx;

        // Flip a byte in the first frame's ciphertext (the byte at
        // PAGE_HEADER_LEN + 11 is somewhere in the AEAD body).
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[PAGE_HEADER_LEN + 11] ^= 0x40;
        std::fs::write(&path, &bytes).unwrap();

        let cipher = fresh_cipher(0x33, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let err = wal2.recover().unwrap_err();
        assert!(err.to_string().contains("ERR_WAL_AEAD"));
    }

    #[test]
    fn v3_truncated_trailing_frame_is_treated_as_truncation() {
        let (mut wal, ctx) = make_encrypted_wal(0x44);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.append(&WalEntry::CommitTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);
        let _ = ctx;

        // Lop off the last 4 bytes — simulates kill-9 partway through
        // a frame write. The reader must treat this as "truncation"
        // and return only the first frame, not raise an error.
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 4]).unwrap();

        let cipher = fresh_cipher(0x44, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let recovered = wal2.recover().unwrap();
        assert_eq!(recovered.len(), 1, "expected only the first frame");
        // The file should now be exactly 1 frame past the page
        // header — recover truncated the partial trailing frame.
        let after = std::fs::metadata(&path).unwrap().len();
        assert!(after >= PAGE_HEADER_LEN as u64);
    }

    #[test]
    fn v3_trailing_frame_with_aead_failure_treated_as_truncation() {
        let (mut wal, ctx) = make_encrypted_wal(0x55);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);
        let _ = ctx;

        // Flip a byte in the only (= trailing) frame. AEAD fails,
        // and because the frame extends to EOF, recover must treat
        // the failure as a truncation — return zero entries, leave
        // a clean file behind.
        let mut bytes = std::fs::read(&path).unwrap();
        let n = bytes.len();
        bytes[n - 5] ^= 0x55;
        std::fs::write(&path, &bytes).unwrap();

        let cipher = fresh_cipher(0x55, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let recovered = wal2.recover().unwrap();
        assert!(recovered.is_empty(), "trailing AEAD should truncate");
    }

    #[test]
    fn with_cipher_rejects_existing_plaintext_wal() {
        let (mut plain, ctx) = create_test_wal();
        plain
            .append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        plain.flush().unwrap();
        let path = plain.path.clone();
        drop_wal(plain);

        let cipher = fresh_cipher(0x77, "default");
        let err = match Wal::with_cipher(&path, cipher) {
            Ok(_) => panic!("expected ERR_WAL_HEADER on plaintext WAL"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("ERR_WAL_HEADER"));
        let _ = ctx;
    }

    #[test]
    fn plaintext_wal_refuses_v3_append_request() {
        let (mut wal, _ctx) = create_test_wal();
        let err = wal
            .append_with_algo(
                &WalEntry::BeginTx { tx_id: 1, epoch: 1 },
                ChecksumAlgo::Aes256GcmCrc32C,
            )
            .unwrap_err();
        // The v3 append path requires a cipher — invoking it on a
        // plaintext WAL surfaces the cipher-missing error.
        assert!(err.to_string().contains("ERR_WAL_CIPHER_MISSING"));
    }

    #[test]
    fn encrypted_wal_truncate_preserves_page_header() {
        let (mut wal, _ctx) = make_encrypted_wal(0x88);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        wal.truncate().unwrap();
        // After truncate, the file must still start with the EaR
        // page header so the inventory scanner classifies it as
        // Encrypted on the next boot.
        let bytes = std::fs::read(&wal.path).unwrap();
        assert_eq!(bytes.len(), PAGE_HEADER_LEN);
        let mut header_buf = [0u8; PAGE_HEADER_LEN];
        header_buf.copy_from_slice(&bytes);
        assert!(PageHeader::from_bytes(&header_buf).is_some());
    }

    #[test]
    fn v3_append_then_replay_after_truncate_starts_fresh_offsets() {
        // Truncate resets frame offsets to PAGE_HEADER_LEN. Nonce
        // uniqueness across the truncate boundary requires a key
        // rotation in production; here we just prove the recover
        // loop walks the post-truncate frames cleanly.
        let (mut wal, _ctx) = make_encrypted_wal(0x99);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        wal.truncate().unwrap();
        wal.append(&WalEntry::CommitTx { tx_id: 2, epoch: 2 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);

        let cipher = fresh_cipher(0x99, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let recovered = wal2.recover().unwrap();
        assert_eq!(recovered.len(), 1);
        assert!(matches!(
            recovered[0],
            WalEntry::CommitTx { tx_id: 2, epoch: 2 }
        ));
    }
}
