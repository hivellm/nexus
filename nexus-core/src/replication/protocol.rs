//! Replication wire protocol
//!
//! All messages are serialized with bincode and validated with CRC32.
//!
//! Format: [message_type:1][length:4][payload:N][crc32:4]

use crate::Result;
use crate::wal::WalEntry;
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Replication message types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationMessageType {
    /// Handshake from replica to master
    Hello = 0x01,
    /// Handshake response from master
    Welcome = 0x02,
    /// Heartbeat ping
    Ping = 0x10,
    /// Heartbeat pong
    Pong = 0x11,
    /// WAL entry
    WalEntry = 0x20,
    /// WAL entry acknowledgment
    WalAck = 0x21,
    /// Request full sync (snapshot)
    RequestSnapshot = 0x30,
    /// Snapshot metadata
    SnapshotMeta = 0x31,
    /// Snapshot data chunk
    SnapshotChunk = 0x32,
    /// Snapshot complete
    SnapshotComplete = 0x33,
    /// Error message
    Error = 0xFF,
}

impl TryFrom<u8> for ReplicationMessageType {
    type Error = crate::Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0x01 => Ok(Self::Hello),
            0x02 => Ok(Self::Welcome),
            0x10 => Ok(Self::Ping),
            0x11 => Ok(Self::Pong),
            0x20 => Ok(Self::WalEntry),
            0x21 => Ok(Self::WalAck),
            0x30 => Ok(Self::RequestSnapshot),
            0x31 => Ok(Self::SnapshotMeta),
            0x32 => Ok(Self::SnapshotChunk),
            0x33 => Ok(Self::SnapshotComplete),
            0xFF => Ok(Self::Error),
            _ => Err(crate::Error::replication(format!(
                "Unknown message type: {}",
                value
            ))),
        }
    }
}

/// Replication messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplicationMessage {
    /// Hello from replica - includes replica ID and last known offset
    Hello {
        replica_id: String,
        last_wal_offset: u64,
        protocol_version: u32,
    },

    /// Welcome from master - includes master ID and current offset
    Welcome {
        master_id: String,
        current_wal_offset: u64,
        requires_full_sync: bool,
    },

    /// Heartbeat ping
    Ping { timestamp: u64 },

    /// Heartbeat pong
    Pong { timestamp: u64 },

    /// WAL entry to replicate
    WalEntry {
        offset: u64,
        epoch: u64,
        entry: WalEntry,
    },

    /// Acknowledgment of WAL entry
    WalAck { offset: u64, success: bool },

    /// Request snapshot transfer
    RequestSnapshot { replica_id: String },

    /// Snapshot metadata
    SnapshotMeta {
        snapshot_id: String,
        total_size: u64,
        chunk_count: u32,
        checksum: u32,
        wal_offset: u64,
    },

    /// Snapshot data chunk
    SnapshotChunk {
        snapshot_id: String,
        chunk_index: u32,
        data: Vec<u8>,
        checksum: u32,
    },

    /// Snapshot transfer complete
    SnapshotComplete { snapshot_id: String, success: bool },

    /// Error message
    Error { code: u32, message: String },
}

impl ReplicationMessage {
    /// Get message type
    pub fn message_type(&self) -> ReplicationMessageType {
        match self {
            Self::Hello { .. } => ReplicationMessageType::Hello,
            Self::Welcome { .. } => ReplicationMessageType::Welcome,
            Self::Ping { .. } => ReplicationMessageType::Ping,
            Self::Pong { .. } => ReplicationMessageType::Pong,
            Self::WalEntry { .. } => ReplicationMessageType::WalEntry,
            Self::WalAck { .. } => ReplicationMessageType::WalAck,
            Self::RequestSnapshot { .. } => ReplicationMessageType::RequestSnapshot,
            Self::SnapshotMeta { .. } => ReplicationMessageType::SnapshotMeta,
            Self::SnapshotChunk { .. } => ReplicationMessageType::SnapshotChunk,
            Self::SnapshotComplete { .. } => ReplicationMessageType::SnapshotComplete,
            Self::Error { .. } => ReplicationMessageType::Error,
        }
    }

    /// Encode message to bytes
    pub fn encode(&self) -> Result<Vec<u8>> {
        let payload = bincode::serialize(self)
            .map_err(|e| crate::Error::replication(format!("Serialization failed: {}", e)))?;

        let mut buf = Vec::with_capacity(1 + 4 + payload.len() + 4);
        buf.push(self.message_type() as u8);
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&payload);

        // CRC32 of type + length + payload
        let mut hasher = Hasher::new();
        hasher.update(&buf);
        let crc = hasher.finalize();
        buf.extend_from_slice(&crc.to_le_bytes());

        Ok(buf)
    }

    /// Decode message from bytes
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < 9 {
            // min: type(1) + length(4) + crc(4)
            return Err(crate::Error::replication("Message too short"));
        }

        // Verify CRC
        let crc_offset = buf.len() - 4;
        let stored_crc = u32::from_le_bytes(buf[crc_offset..].try_into().unwrap());

        let mut hasher = Hasher::new();
        hasher.update(&buf[..crc_offset]);
        let computed_crc = hasher.finalize();

        if stored_crc != computed_crc {
            return Err(crate::Error::replication(format!(
                "CRC mismatch: expected {:x}, got {:x}",
                stored_crc, computed_crc
            )));
        }

        // Extract payload length
        let length = u32::from_le_bytes(buf[1..5].try_into().unwrap()) as usize;
        if buf.len() < 5 + length + 4 {
            return Err(crate::Error::replication("Incomplete message"));
        }

        // Deserialize payload
        let payload = &buf[5..5 + length];
        bincode::deserialize(payload)
            .map_err(|e| crate::Error::replication(format!("Deserialization failed: {}", e)))
    }

    /// Write message to async stream
    pub async fn write_to<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> Result<()> {
        let buf = self.encode()?;
        writer.write_all(&buf).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Read message from async stream
    pub async fn read_from<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Self> {
        // Read header: type(1) + length(4)
        let mut header = [0u8; 5];
        reader.read_exact(&mut header).await?;

        let _msg_type = header[0];
        let length = u32::from_le_bytes(header[1..5].try_into().unwrap()) as usize;

        // Read payload + CRC
        let mut payload_buf = vec![0u8; length + 4];
        reader.read_exact(&mut payload_buf).await?;

        // Combine into full buffer
        let mut full_buf = Vec::with_capacity(5 + payload_buf.len());
        full_buf.extend_from_slice(&header);
        full_buf.extend_from_slice(&payload_buf);

        Self::decode(&full_buf)
    }

    /// Write message to sync stream
    pub fn write_to_sync<W: Write>(&self, writer: &mut W) -> Result<()> {
        let buf = self.encode()?;
        writer.write_all(&buf)?;
        writer.flush()?;
        Ok(())
    }

    /// Read message from sync stream
    pub fn read_from_sync<R: Read>(reader: &mut R) -> Result<Self> {
        // Read header: type(1) + length(4)
        let mut header = [0u8; 5];
        reader.read_exact(&mut header)?;

        let _msg_type = header[0];
        let length = u32::from_le_bytes(header[1..5].try_into().unwrap()) as usize;

        // Read payload + CRC
        let mut payload_buf = vec![0u8; length + 4];
        reader.read_exact(&mut payload_buf)?;

        // Combine into full buffer
        let mut full_buf = Vec::with_capacity(5 + payload_buf.len());
        full_buf.extend_from_slice(&header);
        full_buf.extend_from_slice(&payload_buf);

        Self::decode(&full_buf)
    }
}

/// Protocol version
pub const PROTOCOL_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_encode_decode() {
        let msg = ReplicationMessage::Ping {
            timestamp: 1234567890,
        };

        let encoded = msg.encode().unwrap();
        let decoded = ReplicationMessage::decode(&encoded).unwrap();

        match decoded {
            ReplicationMessage::Ping { timestamp } => {
                assert_eq!(timestamp, 1234567890);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_hello_message() {
        let msg = ReplicationMessage::Hello {
            replica_id: "replica-1".into(),
            last_wal_offset: 1000,
            protocol_version: PROTOCOL_VERSION,
        };

        let encoded = msg.encode().unwrap();
        let decoded = ReplicationMessage::decode(&encoded).unwrap();

        match decoded {
            ReplicationMessage::Hello {
                replica_id,
                last_wal_offset,
                protocol_version,
            } => {
                assert_eq!(replica_id, "replica-1");
                assert_eq!(last_wal_offset, 1000);
                assert_eq!(protocol_version, PROTOCOL_VERSION);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_wal_entry_message() {
        let wal_entry = WalEntry::CreateNode {
            node_id: 42,
            label_bits: 7,
        };

        let msg = ReplicationMessage::WalEntry {
            offset: 100,
            epoch: 1,
            entry: wal_entry,
        };

        let encoded = msg.encode().unwrap();
        let decoded = ReplicationMessage::decode(&encoded).unwrap();

        match decoded {
            ReplicationMessage::WalEntry {
                offset,
                epoch,
                entry,
            } => {
                assert_eq!(offset, 100);
                assert_eq!(epoch, 1);
                match entry {
                    WalEntry::CreateNode {
                        node_id,
                        label_bits,
                    } => {
                        assert_eq!(node_id, 42);
                        assert_eq!(label_bits, 7);
                    }
                    _ => panic!("Wrong entry type"),
                }
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_crc_validation() {
        let msg = ReplicationMessage::Ping { timestamp: 123 };
        let mut encoded = msg.encode().unwrap();

        // Corrupt the data
        encoded[5] ^= 0xFF;

        // Decode should fail
        let result = ReplicationMessage::decode(&encoded);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("CRC"));
    }

    #[test]
    fn test_message_types() {
        assert_eq!(
            ReplicationMessage::Hello {
                replica_id: "".into(),
                last_wal_offset: 0,
                protocol_version: 1
            }
            .message_type(),
            ReplicationMessageType::Hello
        );
        assert_eq!(
            ReplicationMessage::Ping { timestamp: 0 }.message_type(),
            ReplicationMessageType::Ping
        );
        assert_eq!(
            ReplicationMessage::Error {
                code: 0,
                message: "".into()
            }
            .message_type(),
            ReplicationMessageType::Error
        );
    }

    #[test]
    fn test_snapshot_chunk_message() {
        let chunk_data = vec![1, 2, 3, 4, 5];
        let msg = ReplicationMessage::SnapshotChunk {
            snapshot_id: "snap-1".into(),
            chunk_index: 0,
            data: chunk_data.clone(),
            checksum: 12345,
        };

        let encoded = msg.encode().unwrap();
        let decoded = ReplicationMessage::decode(&encoded).unwrap();

        match decoded {
            ReplicationMessage::SnapshotChunk {
                snapshot_id,
                chunk_index,
                data,
                checksum,
            } => {
                assert_eq!(snapshot_id, "snap-1");
                assert_eq!(chunk_index, 0);
                assert_eq!(data, chunk_data);
                assert_eq!(checksum, 12345);
            }
            _ => panic!("Wrong message type"),
        }
    }
}
