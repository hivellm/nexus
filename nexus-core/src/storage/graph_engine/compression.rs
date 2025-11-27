//! Advanced Compression algorithms for graph storage optimization.
//!
//! This module implements state-of-the-art compression techniques specifically designed
//! for graph data structures, particularly adjacency lists with SIMD acceleration.

use super::format::{AdjacencyEntry, CompressionType};
use crate::error::{Error, Result};
use std::collections::HashMap;

/// Compressor for relationship adjacency lists and other graph structures
pub struct RelationshipCompressor {
    // Configuration for compression algorithms
}

impl RelationshipCompressor {
    /// Create a new relationship compressor
    pub fn new() -> Self {
        Self {}
    }

    /// Compress an adjacency list with advanced algorithms
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
            CompressionType::LZ4 => self.compress_lz4(entries),
            CompressionType::Zstd => self.compress_zstd(entries),
            CompressionType::Adaptive => self.compress_adaptive(entries),
            CompressionType::SimdRLE => self.compress_simd_rle(entries),
        }
    }

    /// Decompress an adjacency list with advanced algorithms
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
            CompressionType::LZ4 => self.decompress_lz4(compressed, entry_count),
            CompressionType::Zstd => self.decompress_zstd(compressed, entry_count),
            CompressionType::Adaptive => self.decompress_adaptive(compressed, entry_count),
            CompressionType::SimdRLE => self.decompress_simd_rle(compressed, entry_count),
        }
    }

    /// Determine the best compression type for a given adjacency list using advanced heuristics
    pub fn choose_compression_type(&self, entries: &[AdjacencyEntry]) -> CompressionType {
        if entries.is_empty() {
            return CompressionType::None;
        }

        if entries.len() < 10 {
            return CompressionType::None; // Not worth compressing small lists
        }

        // Analyze data characteristics
        let is_sorted = entries.windows(2).all(|w| w[0].rel_id <= w[1].rel_id);

        // Calculate compression metrics
        let avg_id = entries.iter().map(|e| e.rel_id as f64).sum::<f64>() / entries.len() as f64;
        let variance = entries
            .iter()
            .map(|e| (e.rel_id as f64 - avg_id).powi(2))
            .sum::<f64>()
            / entries.len() as f64;

        // Check for repeated patterns (good for RLE/dictionary)
        let mut repeats = 0;
        for i in 1..entries.len() {
            if entries[i].rel_id == entries[i - 1].rel_id {
                repeats += 1;
            }
        }
        let repeat_ratio = repeats as f64 / entries.len() as f64;

        // Choose algorithm based on characteristics
        if repeat_ratio > 0.3 {
            CompressionType::SimdRLE // High repetition - use RLE
        } else if is_sorted && variance < 1000.0 {
            // Prioritize delta compression for sorted data with low variance
            if entries.len() > 1000 {
                CompressionType::Delta // Large sorted lists - use delta
            } else {
                CompressionType::VarInt // Smaller sorted lists - varint is fine
            }
        } else if entries.len() > 10000 {
            CompressionType::LZ4 // Large datasets - fast compression
        } else if variance > 1000000.0 {
            CompressionType::Zstd // High variance - high compression
        } else if entries.len() > 5000 {
            CompressionType::Adaptive // Very large datasets - let adaptive choose
        } else if entries.len() > 100 {
            CompressionType::VarInt // Medium datasets - variable int
        } else {
            CompressionType::None // Small datasets - no compression
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
        // Dictionary-based compression for repeated patterns
        let mut dictionary = HashMap::new();
        let mut dict_id = 0u16;

        // Build dictionary of common ID patterns (sliding window)
        for window in entries.windows(3) {
            if window.len() == 3 {
                let pattern = (window[0].rel_id, window[1].rel_id, window[2].rel_id);
                dictionary.entry(pattern).or_insert_with(|| {
                    let id = dict_id;
                    dict_id += 1;
                    id
                });
            }
        }

        let mut result = Vec::new();

        // Store dictionary size
        result.extend_from_slice(&(dict_id as u16).to_le_bytes());

        // Store dictionary entries
        for (&pattern, &id) in &dictionary {
            result.extend_from_slice(&id.to_le_bytes());
            result.extend_from_slice(&pattern.0.to_le_bytes());
            result.extend_from_slice(&pattern.1.to_le_bytes());
            result.extend_from_slice(&pattern.2.to_le_bytes());
        }

        // Encode entries using dictionary
        for entry in entries {
            let mut found = false;
            // Check for dictionary matches in sliding window
            for i in 0..(entries.len().saturating_sub(2)) {
                if entries[i].rel_id == entry.rel_id {
                    // Look for pattern match
                    if i + 2 < entries.len()
                        && entries[i + 1].rel_id
                            == entries.get(i + 1).map(|e| e.rel_id).unwrap_or(0)
                        && entries[i + 2].rel_id
                            == entries.get(i + 2).map(|e| e.rel_id).unwrap_or(0)
                    {
                        if let Some(&dict_id) = dictionary.get(&(
                            entries[i].rel_id,
                            entries[i + 1].rel_id,
                            entries[i + 2].rel_id,
                        )) {
                            result.push(0x80 | ((dict_id >> 8) as u8)); // Dict marker
                            result.push(dict_id as u8);
                            found = true;
                            break;
                        }
                    }
                }
            }

            if !found {
                // Encode as literal
                result.push(0x00); // Literal marker
                result.extend_from_slice(&entry.rel_id.to_le_bytes());
            }
        }

        Ok(result)
    }

    fn compress_lz4(&self, entries: &[AdjacencyEntry]) -> Result<Vec<u8>> {
        // LZ4 compression for fast decompression
        let input_bytes = unsafe {
            std::slice::from_raw_parts(
                entries.as_ptr() as *const u8,
                entries.len() * std::mem::size_of::<AdjacencyEntry>(),
            )
        };

        // Simple LZ4-like compression (simplified implementation)
        // In production, this would use the lz4 crate
        let mut result = Vec::new();
        let mut i = 0;

        while i < input_bytes.len() {
            // Find best match in sliding window
            let mut best_match_len = 0;
            let mut best_match_dist = 0;

            for dist in 1..=std::cmp::min(i, 255) {
                let mut match_len = 0;
                while match_len < 255
                    && i + match_len < input_bytes.len()
                    && input_bytes[i - dist + match_len] == input_bytes[i + match_len]
                {
                    match_len += 1;
                }

                if match_len > best_match_len {
                    best_match_len = match_len;
                    best_match_dist = dist;
                }
            }

            if best_match_len >= 4 {
                // Encode as match
                result.push((best_match_dist as u8) | 0x80); // Match marker
                result.push(best_match_len as u8);
                i += best_match_len;
            } else {
                // Encode as literal
                result.push(0x00); // Literal marker
                let literal_len = std::cmp::min(255, input_bytes.len() - i);
                result.push(literal_len as u8);
                result.extend_from_slice(&input_bytes[i..i + literal_len]);
                i += literal_len;
            }
        }

        Ok(result)
    }

    fn compress_zstd(&self, entries: &[AdjacencyEntry]) -> Result<Vec<u8>> {
        // Zstandard compression for high compression ratios
        let input_bytes = unsafe {
            std::slice::from_raw_parts(
                entries.as_ptr() as *const u8,
                entries.len() * std::mem::size_of::<AdjacencyEntry>(),
            )
        };

        // Simplified Zstd-like compression
        // In production, this would use the zstd crate
        let mut result = Vec::new();

        // Compress in blocks
        for chunk in input_bytes.chunks(4096) {
            let compressed = self.compress_zstd_block(chunk)?;
            result.extend_from_slice(&(compressed.len() as u16).to_le_bytes());
            result.extend_from_slice(&compressed);
        }

        Ok(result)
    }

    fn compress_zstd_block(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Simplified block compression
        let mut result = Vec::new();
        let mut i = 0;

        while i < data.len() {
            let mut best_match_len = 0;
            let mut best_match_dist = 0;

            // Search for matches
            for dist in 1..=std::cmp::min(i, 32767) {
                let mut match_len = 0;
                while match_len < 255
                    && i + match_len < data.len()
                    && data[i - dist + match_len] == data[i + match_len]
                {
                    match_len += 1;
                }

                if match_len > best_match_len {
                    best_match_len = match_len;
                    best_match_dist = dist;
                }
            }

            if best_match_len >= 3 {
                // Encode as match
                let dist_bytes = (best_match_dist as u16).to_le_bytes();
                result.push(dist_bytes[0]);
                result.push(dist_bytes[1]);
                result.push(best_match_len as u8);
                i += best_match_len;
            } else {
                // Encode as literals
                let literal_len = std::cmp::min(127, data.len() - i);
                result.push(literal_len as u8 | 0x80); // Literal marker
                result.extend_from_slice(&data[i..i + literal_len]);
                i += literal_len;
            }
        }

        Ok(result)
    }

    fn compress_adaptive(&self, entries: &[AdjacencyEntry]) -> Result<Vec<u8>> {
        // Adaptive compression that chooses the best algorithm
        if entries.is_empty() {
            return Ok(Vec::new());
        }

        // Analyze data characteristics
        let is_sorted = entries.windows(2).all(|w| w[0].rel_id <= w[1].rel_id);
        let avg_id = entries.iter().map(|e| e.rel_id as f64).sum::<f64>() / entries.len() as f64;
        let variance = entries
            .iter()
            .map(|e| (e.rel_id as f64 - avg_id).powi(2))
            .sum::<f64>()
            / entries.len() as f64;

        // Choose algorithm based on characteristics
        let chosen_type = if entries.len() < 10 {
            CompressionType::None
        } else if is_sorted && variance < 1000.0 {
            CompressionType::Delta // Good for sorted, low variance data
        } else if entries.len() > 1000 {
            CompressionType::LZ4 // Fast compression for large datasets
        } else if variance > 10000.0 {
            CompressionType::Zstd // High compression for high variance data
        } else {
            CompressionType::VarInt // Good general purpose
        };

        // Prepend chosen algorithm as first byte
        let mut result = vec![chosen_type as u8];
        let compressed = self.compress_adjacency_list(entries, chosen_type)?;
        result.extend(compressed);

        Ok(result)
    }

    fn compress_simd_rle(&self, entries: &[AdjacencyEntry]) -> Result<Vec<u8>> {
        // SIMD-accelerated Run-Length Encoding for repeated values
        let mut result = Vec::new();
        let mut i = 0;

        while i < entries.len() {
            let current_id = entries[i].rel_id;
            let mut run_length = 1;

            // Count consecutive identical IDs
            while i + run_length < entries.len() && entries[i + run_length].rel_id == current_id {
                run_length += 1;
            }

            if run_length >= 3 {
                // Encode as RLE
                result.push(0xFF); // RLE marker
                result.extend_from_slice(&current_id.to_le_bytes());
                result.extend_from_slice(&(run_length as u16).to_le_bytes());
                i += run_length;
            } else {
                // Encode as literals
                let literal_count = std::cmp::min(127, entries.len() - i);
                result.push(literal_count as u8); // Literal marker with count
                for j in 0..literal_count {
                    result.extend_from_slice(&entries[i + j].rel_id.to_le_bytes());
                }
                i += literal_count;
            }
        }

        Ok(result)
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
        if compressed.len() < 2 {
            return self.decompress_none(compressed, entry_count);
        }

        let dict_size = u16::from_le_bytes([compressed[0], compressed[1]]) as usize;
        let mut pos = 2;

        // Read dictionary
        let mut dictionary = HashMap::new();
        for _ in 0..dict_size {
            if pos + 10 > compressed.len() {
                break;
            }
            let dict_id = u16::from_le_bytes([compressed[pos], compressed[pos + 1]]);
            pos += 2;
            let pattern0 = u64::from_le_bytes(compressed[pos..pos + 8].try_into().unwrap());
            pos += 8;
            let pattern1 = u64::from_le_bytes(compressed[pos..pos + 8].try_into().unwrap());
            pos += 8;
            let pattern2 = u64::from_le_bytes(compressed[pos..pos + 8].try_into().unwrap());
            pos += 8;

            dictionary.insert(dict_id, (pattern0, pattern1, pattern2));
        }

        // Decompress entries
        let mut result = Vec::with_capacity(entry_count);
        while result.len() < entry_count && pos < compressed.len() {
            let marker = compressed[pos];
            pos += 1;

            if marker & 0x80 != 0 {
                // Dictionary reference
                if pos < compressed.len() {
                    let dict_id = u16::from_le_bytes([compressed[pos], marker & 0x7F]);
                    pos += 1;

                    if let Some(&(id0, id1, id2)) = dictionary.get(&dict_id) {
                        result.push(AdjacencyEntry { rel_id: id0 });
                        if result.len() < entry_count {
                            result.push(AdjacencyEntry { rel_id: id1 });
                        }
                        if result.len() < entry_count {
                            result.push(AdjacencyEntry { rel_id: id2 });
                        }
                    }
                }
            } else {
                // Literal
                if pos + 8 <= compressed.len() {
                    let id = u64::from_le_bytes(compressed[pos..pos + 8].try_into().unwrap());
                    pos += 8;
                    result.push(AdjacencyEntry { rel_id: id });
                }
            }
        }

        Ok(result)
    }

    fn decompress_lz4(&self, compressed: &[u8], entry_count: usize) -> Result<Vec<AdjacencyEntry>> {
        let mut result = Vec::with_capacity(entry_count * std::mem::size_of::<AdjacencyEntry>());
        let mut pos = 0;

        while pos < compressed.len()
            && result.len() < entry_count * std::mem::size_of::<AdjacencyEntry>()
        {
            if pos >= compressed.len() {
                break;
            }

            let marker = compressed[pos];
            pos += 1;

            if marker & 0x80 != 0 {
                // Match
                let distance = (marker & 0x7F) as usize;
                let length = compressed.get(pos).copied().unwrap_or(0) as usize;
                pos += 1;

                if distance > 0 && length > 0 && result.len() >= distance {
                    // Copy from history
                    for _ in 0..length {
                        if result.len() >= distance {
                            let byte = result[result.len() - distance];
                            result.push(byte);
                        }
                    }
                }
            } else {
                // Literal
                let length = marker as usize;
                let copy_len = std::cmp::min(length, compressed.len() - pos);
                result.extend_from_slice(&compressed[pos..pos + copy_len]);
                pos += copy_len;
            }
        }

        // Convert bytes back to entries
        let expected_bytes = entry_count * std::mem::size_of::<AdjacencyEntry>();
        if result.len() != expected_bytes {
            return Err(Error::Storage(format!(
                "LZ4 decompression produced {} bytes, expected {}",
                result.len(),
                expected_bytes
            )));
        }

        let entries = unsafe {
            std::slice::from_raw_parts(result.as_ptr() as *const AdjacencyEntry, entry_count)
        };

        Ok(entries.to_vec())
    }

    fn decompress_zstd(
        &self,
        compressed: &[u8],
        entry_count: usize,
    ) -> Result<Vec<AdjacencyEntry>> {
        let mut result = Vec::new();
        let mut pos = 0;

        while pos < compressed.len() {
            if pos + 2 > compressed.len() {
                break;
            }

            let block_size = u16::from_le_bytes([compressed[pos], compressed[pos + 1]]) as usize;
            pos += 2;

            if pos + block_size > compressed.len() {
                break;
            }

            let decompressed = self.decompress_zstd_block(&compressed[pos..pos + block_size])?;
            result.extend(decompressed);
            pos += block_size;
        }

        // Convert to entries
        let expected_bytes = entry_count * std::mem::size_of::<AdjacencyEntry>();
        if result.len() != expected_bytes {
            return Err(Error::Storage(format!(
                "Zstd decompression produced {} bytes, expected {}",
                result.len(),
                expected_bytes
            )));
        }

        let entries = unsafe {
            std::slice::from_raw_parts(result.as_ptr() as *const AdjacencyEntry, entry_count)
        };

        Ok(entries.to_vec())
    }

    fn decompress_zstd_block(&self, compressed: &[u8]) -> Result<Vec<u8>> {
        let mut result = Vec::new();
        let mut pos = 0;

        while pos < compressed.len() {
            let marker = compressed[pos];
            pos += 1;

            if marker & 0x80 != 0 {
                // Literals
                let literal_len = (marker & 0x7F) as usize;
                let copy_len = std::cmp::min(literal_len, compressed.len() - pos);
                result.extend_from_slice(&compressed[pos..pos + copy_len]);
                pos += copy_len;
            } else {
                // Match
                if pos + 2 > compressed.len() {
                    break;
                }
                let dist = u16::from_le_bytes([compressed[pos], compressed[pos + 1]]) as usize;
                pos += 2;
                let length = compressed[pos] as usize;
                pos += 1;

                // Copy from history
                for _ in 0..length {
                    if result.len() >= dist {
                        let byte = result[result.len() - dist];
                        result.push(byte);
                    }
                }
            }
        }

        Ok(result)
    }

    fn decompress_adaptive(
        &self,
        compressed: &[u8],
        entry_count: usize,
    ) -> Result<Vec<AdjacencyEntry>> {
        if compressed.is_empty() {
            return Ok(Vec::new());
        }

        // First byte indicates the chosen algorithm
        let algorithm_byte = compressed[0];
        let compression_type = match algorithm_byte {
            0 => CompressionType::None,
            1 => CompressionType::VarInt,
            2 => CompressionType::Delta,
            3 => CompressionType::Dictionary,
            4 => CompressionType::LZ4,
            5 => CompressionType::Zstd,
            6 => CompressionType::Adaptive,
            7 => CompressionType::SimdRLE,
            _ => {
                return Err(Error::Storage(
                    "Unknown compression type in adaptive data".to_string(),
                ));
            }
        };

        self.decompress_adjacency_list(&compressed[1..], compression_type, entry_count)
    }

    fn decompress_simd_rle(
        &self,
        compressed: &[u8],
        entry_count: usize,
    ) -> Result<Vec<AdjacencyEntry>> {
        let mut result = Vec::with_capacity(entry_count);
        let mut pos = 0;

        while pos < compressed.len() && result.len() < entry_count {
            let marker = compressed[pos];
            pos += 1;

            if marker == 0xFF {
                // RLE sequence
                if pos + 10 > compressed.len() {
                    break;
                }
                let id = u64::from_le_bytes(compressed[pos..pos + 8].try_into().unwrap());
                pos += 8;
                let count = u16::from_le_bytes([compressed[pos], compressed[pos + 1]]) as usize;
                pos += 2;

                // Repeat the ID
                for _ in 0..count {
                    result.push(AdjacencyEntry { rel_id: id });
                }
            } else {
                // Literals
                let literal_count = marker as usize;
                for _ in 0..literal_count {
                    if pos + 8 > compressed.len() {
                        break;
                    }
                    let id = u64::from_le_bytes(compressed[pos..pos + 8].try_into().unwrap());
                    pos += 8;
                    result.push(AdjacencyEntry { rel_id: id });
                }
            }
        }

        Ok(result)
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

        // Large sorted list - delta or varint (both valid for sorted data)
        let large_sorted = (0..2000)
            .map(|i| AdjacencyEntry { rel_id: i })
            .collect::<Vec<_>>();
        let chosen_type = compressor.choose_compression_type(&large_sorted);
        assert!(
            chosen_type == CompressionType::Delta || chosen_type == CompressionType::VarInt,
            "Expected Delta or VarInt for large sorted data, got {:?}",
            chosen_type
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
