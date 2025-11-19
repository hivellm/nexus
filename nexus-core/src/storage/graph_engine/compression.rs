//! Compression algorithms for graph storage optimization.
//!
//! This module implements compression techniques specifically designed
//! for graph data structures, particularly adjacency lists.

use super::format::{AdjacencyEntry, CompressionType};
use crate::error::{Error, Result};

/// Compressor for relationship adjacency lists and other graph structures
pub struct RelationshipCompressor {
    // Configuration for compression algorithms
}

impl RelationshipCompressor {
    /// Create a new relationship compressor
    pub fn new() -> Self {
        Self {}
    }

    /// Compress an adjacency list
    pub fn compress_adjacency_list(
        &self,
        entries: &[AdjacencyEntry],
        compression_type: CompressionType,
    ) -> Result<Vec<u8>> {
        match compression_type {
            CompressionType::None => self.compress_none(entries),
            CompressionType::VarInt => self.compress_varint(entries),
            CompressionType::Delta => self.compress_delta(entries),
            CompressionType::Dictionary => self.compress_dictionary(entries),
        }
    }

    /// Decompress an adjacency list
    pub fn decompress_adjacency_list(
        &self,
        compressed: &[u8],
        compression_type: CompressionType,
        entry_count: usize,
    ) -> Result<Vec<AdjacencyEntry>> {
        match compression_type {
            CompressionType::None => self.decompress_none(compressed, entry_count),
            CompressionType::VarInt => self.decompress_varint(compressed, entry_count),
            CompressionType::Delta => self.decompress_delta(compressed, entry_count),
            CompressionType::Dictionary => self.decompress_dictionary(compressed, entry_count),
        }
    }

    /// Determine the best compression type for a given adjacency list
    pub fn choose_compression_type(&self, entries: &[AdjacencyEntry]) -> CompressionType {
        if entries.is_empty() {
            return CompressionType::None;
        }

        if entries.len() < 10 {
            return CompressionType::None; // Not worth compressing small lists
        }

        // Check if IDs are sorted (good for delta compression)
        let is_sorted = entries.windows(2).all(|w| w[0].rel_id <= w[1].rel_id);

        if is_sorted && entries.len() > 1000 {
            CompressionType::Delta // Best for large sorted lists
        } else if entries.len() > 100 {
            CompressionType::VarInt // Good general-purpose compression
        } else {
            CompressionType::None
        }
    }

    // Compression implementations

    fn compress_none(&self, entries: &[AdjacencyEntry]) -> Result<Vec<u8>> {
        // No compression - just copy the bytes
        let bytes = unsafe {
            std::slice::from_raw_parts(
                entries.as_ptr() as *const u8,
                entries.len() * std::mem::size_of::<AdjacencyEntry>(),
            )
        };
        Ok(bytes.to_vec())
    }

    fn compress_varint(&self, entries: &[AdjacencyEntry]) -> Result<Vec<u8>> {
        let mut result = Vec::new();

        for entry in entries {
            // Simple variable-length encoding for relationship IDs
            self.encode_varint(entry.rel_id, &mut result)?;
        }

        Ok(result)
    }

    fn compress_delta(&self, entries: &[AdjacencyEntry]) -> Result<Vec<u8>> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();

        // Store first ID as-is
        self.encode_varint(entries[0].rel_id, &mut result)?;

        // Store deltas from previous ID
        for i in 1..entries.len() {
            let delta = entries[i].rel_id.saturating_sub(entries[i - 1].rel_id);
            self.encode_varint(delta, &mut result)?;
        }

        Ok(result)
    }

    fn compress_dictionary(&self, entries: &[AdjacencyEntry]) -> Result<Vec<u8>> {
        // TODO: Implement dictionary-based compression
        // This would build a dictionary of common ID patterns
        self.compress_none(entries) // Fallback for now
    }

    // Decompression implementations

    fn decompress_none(
        &self,
        compressed: &[u8],
        entry_count: usize,
    ) -> Result<Vec<AdjacencyEntry>> {
        let expected_size = entry_count * std::mem::size_of::<AdjacencyEntry>();

        if compressed.len() != expected_size {
            return Err(Error::Storage(format!(
                "Compressed size {} does not match expected size {} for {} entries",
                compressed.len(),
                expected_size,
                entry_count
            )));
        }

        let entries = unsafe {
            std::slice::from_raw_parts(compressed.as_ptr() as *const AdjacencyEntry, entry_count)
        };

        Ok(entries.to_vec())
    }

    fn decompress_varint(
        &self,
        compressed: &[u8],
        entry_count: usize,
    ) -> Result<Vec<AdjacencyEntry>> {
        let mut result = Vec::with_capacity(entry_count);
        let mut pos = 0;

        for _ in 0..entry_count {
            let (id, new_pos) = self.decode_varint(compressed, pos)?;
            result.push(AdjacencyEntry { rel_id: id });
            pos = new_pos;
        }

        Ok(result)
    }

    fn decompress_delta(
        &self,
        compressed: &[u8],
        entry_count: usize,
    ) -> Result<Vec<AdjacencyEntry>> {
        if entry_count == 0 {
            return Ok(Vec::new());
        }

        let mut result = Vec::with_capacity(entry_count);
        let mut pos = 0;

        // Read first ID
        let (mut current_id, new_pos) = self.decode_varint(compressed, pos)?;
        result.push(AdjacencyEntry { rel_id: current_id });
        pos = new_pos;

        // Read deltas and reconstruct IDs
        for _ in 1..entry_count {
            let (delta, new_pos) = self.decode_varint(compressed, pos)?;
            current_id = current_id.saturating_add(delta);
            result.push(AdjacencyEntry { rel_id: current_id });
            pos = new_pos;
        }

        Ok(result)
    }

    fn decompress_dictionary(
        &self,
        compressed: &[u8],
        entry_count: usize,
    ) -> Result<Vec<AdjacencyEntry>> {
        // TODO: Implement dictionary decompression
        self.decompress_none(compressed, entry_count) // Fallback for now
    }

    // Helper methods for variable-length encoding

    fn encode_varint(&self, value: u64, output: &mut Vec<u8>) -> Result<()> {
        let mut val = value;

        loop {
            let mut byte = (val & 0x7F) as u8;
            val >>= 7;

            if val != 0 {
                byte |= 0x80;
            }

            output.push(byte);

            if val == 0 {
                break;
            }
        }

        Ok(())
    }

    fn decode_varint(&self, input: &[u8], mut pos: usize) -> Result<(u64, usize)> {
        let mut result = 0u64;
        let mut shift = 0;

        loop {
            if pos >= input.len() {
                return Err(Error::Storage("Unexpected end of varint data".to_string()));
            }

            let byte = input[pos];
            pos += 1;

            result |= ((byte & 0x7F) as u64) << shift;

            if (byte & 0x80) == 0 {
                break;
            }

            shift += 7;
            if shift >= 64 {
                return Err(Error::Storage("Varint too long".to_string()));
            }
        }

        Ok((result, pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_compression() {
        let compressor = RelationshipCompressor::new();

        let entries = vec![
            AdjacencyEntry { rel_id: 1 },
            AdjacencyEntry { rel_id: 2 },
            AdjacencyEntry { rel_id: 3 },
        ];

        let compressed = compressor
            .compress_adjacency_list(&entries, CompressionType::None)
            .unwrap();
        let decompressed = compressor
            .decompress_adjacency_list(&compressed, CompressionType::None, entries.len())
            .unwrap();

        assert_eq!(entries, decompressed);
    }

    #[test]
    fn test_varint_compression() {
        let compressor = RelationshipCompressor::new();

        let entries = vec![
            AdjacencyEntry { rel_id: 1 },
            AdjacencyEntry { rel_id: 300 }, // Requires multiple bytes
            AdjacencyEntry { rel_id: 70000 }, // Large number
        ];

        let compressed = compressor
            .compress_adjacency_list(&entries, CompressionType::VarInt)
            .unwrap();
        let decompressed = compressor
            .decompress_adjacency_list(&compressed, CompressionType::VarInt, entries.len())
            .unwrap();

        assert_eq!(entries, decompressed);
    }

    #[test]
    fn test_delta_compression() {
        let compressor = RelationshipCompressor::new();

        let entries = vec![
            AdjacencyEntry { rel_id: 100 },
            AdjacencyEntry { rel_id: 105 }, // delta = 5
            AdjacencyEntry { rel_id: 110 }, // delta = 5
            AdjacencyEntry { rel_id: 120 }, // delta = 10
        ];

        let compressed = compressor
            .compress_adjacency_list(&entries, CompressionType::Delta)
            .unwrap();
        let decompressed = compressor
            .decompress_adjacency_list(&compressed, CompressionType::Delta, entries.len())
            .unwrap();

        assert_eq!(entries, decompressed);
    }

    #[test]
    fn test_compression_type_selection() {
        let compressor = RelationshipCompressor::new();

        // Small list - no compression
        let small_list = vec![AdjacencyEntry { rel_id: 1 }];
        assert_eq!(
            compressor.choose_compression_type(&small_list),
            CompressionType::None
        );

        // Medium sorted list - varint
        let medium_sorted = (0..200)
            .map(|i| AdjacencyEntry { rel_id: i })
            .collect::<Vec<_>>();
        assert_eq!(
            compressor.choose_compression_type(&medium_sorted),
            CompressionType::VarInt
        );

        // Large sorted list - delta
        let large_sorted = (0..2000)
            .map(|i| AdjacencyEntry { rel_id: i })
            .collect::<Vec<_>>();
        assert_eq!(
            compressor.choose_compression_type(&large_sorted),
            CompressionType::Delta
        );
    }

    #[test]
    fn test_varint_encoding() {
        let compressor = RelationshipCompressor::new();

        // Test small numbers
        let mut output = Vec::new();
        compressor.encode_varint(42, &mut output).unwrap();
        let (decoded, _) = compressor.decode_varint(&output, 0).unwrap();
        assert_eq!(decoded, 42);

        // Test large numbers
        output.clear();
        compressor.encode_varint(123456789, &mut output).unwrap();
        let (decoded, _) = compressor.decode_varint(&output, 0).unwrap();
        assert_eq!(decoded, 123456789);
    }
}
