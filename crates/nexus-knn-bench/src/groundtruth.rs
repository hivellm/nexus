//! Brute-force ground-truth top-k computation with disk caching.
//!
//! HNSW recall numbers are only meaningful relative to the true
//! nearest neighbours. We compute those exactly with a brute-force
//! scan in cosine distance — same metric the engine uses
//! ([`nexus_core::index::KnnIndex`] is built on `DistSimdCosine`).
//!
//! For SIFT1M (1 000 000 base × 10 000 queries × 128 dims) the brute-
//! force scan takes a few minutes on commodity hardware. We
//! therefore serialise the result to disk on first run and reuse it
//! on every subsequent invocation. The on-disk format is a tiny
//! header plus a flat little-endian `u32` table of `[query, rank] →
//! base_id`; we hash the corpus into the filename so the cache
//! invalidates automatically when the source data changes.

use std::collections::hash_map::DefaultHasher;
use std::fs::{File, create_dir_all};
use std::hash::{Hash, Hasher};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::Vector;

/// Ground-truth top-k assignment per query.
#[derive(Debug, Clone)]
pub struct Groundtruth {
    /// `top_k[query_index]` = ids of the `k` nearest base vectors,
    /// sorted nearest first.
    pub top_k: Vec<Vec<u32>>,
    pub k: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum GroundtruthError {
    #[error("corpus is empty")]
    EmptyCorpus,
    #[error("query/base dimension mismatch: base={base}, query={query}")]
    DimensionMismatch { base: usize, query: usize },
    #[error("k={k} cannot exceed base size {base_count}")]
    KExceedsBase { k: usize, base_count: usize },
    #[error("io error in ground-truth cache {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("ground-truth cache file {path} is corrupt: {message}")]
    CorruptCache { path: PathBuf, message: String },
}

const CACHE_MAGIC: u32 = 0x4E4B4742; // "NKGB"  Nexus KNN Groundtruth Binary
const CACHE_VERSION: u16 = 1;

impl Groundtruth {
    /// Compute the brute-force top-k from scratch. Costs
    /// `O(|base| * |queries| * dim)` floating-point ops. Use
    /// [`Self::compute_with_cache`] in production runs.
    pub fn compute(
        base: &[Vector],
        queries: &[Vector],
        k: usize,
    ) -> Result<Self, GroundtruthError> {
        if base.is_empty() || queries.is_empty() {
            return Err(GroundtruthError::EmptyCorpus);
        }
        let dim = base[0].len();
        if queries[0].len() != dim {
            return Err(GroundtruthError::DimensionMismatch {
                base: dim,
                query: queries[0].len(),
            });
        }
        if k > base.len() {
            return Err(GroundtruthError::KExceedsBase {
                k,
                base_count: base.len(),
            });
        }

        // Pre-normalise the base set so cosine distance reduces to
        // `1 - dot(a_n, b_n)` — same shortcut `DistSimdCosine` takes
        // internally. We don't mutate the caller's slice; the
        // normalised copies live for the duration of the scan only.
        let base_norm: Vec<Vector> = base.iter().map(|v| normalise(v)).collect();

        let mut out = Vec::with_capacity(queries.len());
        for q in queries {
            if q.len() != dim {
                return Err(GroundtruthError::DimensionMismatch {
                    base: dim,
                    query: q.len(),
                });
            }
            let q_norm = normalise(q);
            let mut scored: Vec<(f32, u32)> = base_norm
                .iter()
                .enumerate()
                .map(|(i, b)| (cosine_distance_normalised(&q_norm, b), i as u32))
                .collect();
            // Partial sort is enough; we only need the smallest `k`.
            scored.select_nth_unstable_by(k - 1, |a, b| {
                a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
            });
            scored.truncate(k);
            scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            out.push(scored.into_iter().map(|(_, id)| id).collect());
        }
        Ok(Self { top_k: out, k })
    }

    /// Compute top-k using a disk cache keyed by `(base, queries, k)`.
    /// First call writes the cache; later calls read it back.
    pub fn compute_with_cache(
        base: &[Vector],
        queries: &[Vector],
        k: usize,
        cache_dir: &Path,
    ) -> Result<Self, GroundtruthError> {
        let key = corpus_key(base, queries, k);
        let cache_path = cache_dir.join(format!("groundtruth-{key:016x}.nkgb"));
        if cache_path.exists() {
            match Self::read_cache(&cache_path) {
                Ok(loaded) if loaded.k == k => return Ok(loaded),
                Ok(_) | Err(_) => {
                    // Stale or corrupt cache — fall through to recompute.
                }
            }
        }
        let computed = Self::compute(base, queries, k)?;
        if let Err(e) = computed.write_cache(&cache_path) {
            tracing::warn!(
                path = %cache_path.display(),
                error = %e,
                "groundtruth cache write failed; continuing without cache"
            );
        }
        Ok(computed)
    }

    fn write_cache(&self, path: &Path) -> Result<(), GroundtruthError> {
        if let Some(parent) = path.parent() {
            create_dir_all(parent).map_err(|e| GroundtruthError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let file = File::create(path).map_err(|e| GroundtruthError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let mut writer = BufWriter::new(file);
        writer
            .write_u32::<LittleEndian>(CACHE_MAGIC)
            .and_then(|_| writer.write_u16::<LittleEndian>(CACHE_VERSION))
            .and_then(|_| writer.write_u32::<LittleEndian>(self.top_k.len() as u32))
            .and_then(|_| writer.write_u32::<LittleEndian>(self.k as u32))
            .map_err(|e| GroundtruthError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
        for row in &self.top_k {
            for id in row {
                writer
                    .write_u32::<LittleEndian>(*id)
                    .map_err(|e| GroundtruthError::Io {
                        path: path.to_path_buf(),
                        source: e,
                    })?;
            }
        }
        writer.flush().map_err(|e| GroundtruthError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        Ok(())
    }

    fn read_cache(path: &Path) -> Result<Self, GroundtruthError> {
        let file = File::open(path).map_err(|e| GroundtruthError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let mut reader = BufReader::new(file);
        let magic = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| GroundtruthError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
        if magic != CACHE_MAGIC {
            return Err(GroundtruthError::CorruptCache {
                path: path.to_path_buf(),
                message: format!("magic {magic:#010x} != {CACHE_MAGIC:#010x}"),
            });
        }
        let version = reader
            .read_u16::<LittleEndian>()
            .map_err(|e| GroundtruthError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
        if version != CACHE_VERSION {
            return Err(GroundtruthError::CorruptCache {
                path: path.to_path_buf(),
                message: format!("version {version} != {CACHE_VERSION}"),
            });
        }
        let queries = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| GroundtruthError::Io {
                path: path.to_path_buf(),
                source: e,
            })? as usize;
        let k = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| GroundtruthError::Io {
                path: path.to_path_buf(),
                source: e,
            })? as usize;
        let mut top_k = Vec::with_capacity(queries);
        for _ in 0..queries {
            let mut row = vec![0u32; k];
            for slot in row.iter_mut() {
                *slot = reader
                    .read_u32::<LittleEndian>()
                    .map_err(|e| GroundtruthError::Io {
                        path: path.to_path_buf(),
                        source: e,
                    })?;
            }
            top_k.push(row);
        }
        // Trailing bytes are a strong corruption signal.
        let mut tail = [0u8; 1];
        if reader.read(&mut tail).map(|n| n > 0).unwrap_or(false) {
            return Err(GroundtruthError::CorruptCache {
                path: path.to_path_buf(),
                message: "trailing bytes after declared payload".into(),
            });
        }
        Ok(Self { top_k, k })
    }
}

fn normalise(v: &[f32]) -> Vector {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < f32::EPSILON {
        return v.to_vec();
    }
    v.iter().map(|x| x / norm).collect()
}

fn cosine_distance_normalised(a: &[f32], b: &[f32]) -> f32 {
    // Both inputs are unit vectors -> cosine distance = 1 - a·b.
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    1.0 - dot
}

fn corpus_key(base: &[Vector], queries: &[Vector], k: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    base.len().hash(&mut hasher);
    queries.len().hash(&mut hasher);
    k.hash(&mut hasher);
    if let Some(first) = base.first() {
        first.len().hash(&mut hasher);
        for x in first {
            x.to_bits().hash(&mut hasher);
        }
    }
    if let Some(last) = base.last() {
        for x in last {
            x.to_bits().hash(&mut hasher);
        }
    }
    if let Some(first) = queries.first() {
        for x in first {
            x.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn unit_axes() -> Vec<Vector> {
        vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![1.0, 1.0, 0.0],
        ]
    }

    #[test]
    fn brute_force_returns_self_for_aligned_queries() {
        let base = unit_axes();
        let queries = vec![vec![1.0, 0.0, 0.0]];
        let gt = Groundtruth::compute(&base, &queries, 2).expect("gt");
        // Index 0 is the identical vector — must come first. Index 3
        // (1,1,0) is the next-closest under cosine.
        assert_eq!(gt.top_k[0][0], 0);
        assert_eq!(gt.top_k[0][1], 3);
    }

    #[test]
    fn cache_roundtrip_preserves_top_k() {
        let dir = TempDir::new().expect("tempdir");
        let base = unit_axes();
        let queries = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        let first = Groundtruth::compute_with_cache(&base, &queries, 2, dir.path()).expect("first");
        let cached =
            Groundtruth::compute_with_cache(&base, &queries, 2, dir.path()).expect("cached");
        assert_eq!(first.top_k, cached.top_k);
        // Cache file exists.
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().into_string().unwrap_or_default())
            .filter(|n| n.starts_with("groundtruth-"))
            .collect();
        assert_eq!(entries.len(), 1, "expected exactly one cache file");
    }

    #[test]
    fn k_exceeding_base_is_rejected() {
        let base = unit_axes();
        let queries = vec![vec![1.0, 0.0, 0.0]];
        let err = Groundtruth::compute(&base, &queries, base.len() + 1).unwrap_err();
        assert!(matches!(err, GroundtruthError::KExceedsBase { .. }));
    }

    #[test]
    fn dimension_mismatch_is_rejected() {
        let base = unit_axes();
        let queries = vec![vec![1.0, 0.0]]; // wrong dim
        let err = Groundtruth::compute(&base, &queries, 1).unwrap_err();
        assert!(matches!(err, GroundtruthError::DimensionMismatch { .. }));
    }
}
