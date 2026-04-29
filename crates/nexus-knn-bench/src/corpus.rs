//! Public corpus loaders.
//!
//! Two formats are supported:
//!
//! * **fvecs / ivecs** — the binary layout INRIA ships SIFT1M in
//!   (and most other ANN benchmarks reuse). Each record is a 4-byte
//!   little-endian dimension followed by `dim * 4` bytes of `f32`
//!   (or `i32` for the `ivecs` ground-truth files). See
//!   <http://corpus-texmex.irisa.fr/> for the canonical reference.
//!
//! * **GloVe text** — one vector per line: token, then `dim`
//!   space-separated `f32`s. Picked up directly from the Stanford
//!   distribution (<https://nlp.stanford.edu/projects/glove/>).
//!
//! Both formats stream end-to-end without holding the file in memory.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::Vector;

/// Which public corpus a [`Corpus`] was built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CorpusKind {
    /// SIFT1M (128-d `f32`). 1 000 000 base + 10 000 query vectors,
    /// distributed in `fvecs` format with `ivecs` ground truth.
    Sift,
    /// GloVe-200d English wikipedia/gigaword tokens. Distributed as
    /// space-separated text.
    Glove,
    /// Synthetic data — used by the unit tests.
    Synthetic,
}

/// Wire format of the source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CorpusFormat {
    /// `fvecs` binary (little-endian).
    Fvecs,
    /// GloVe whitespace-separated text.
    GloveText,
}

/// In-memory corpus. Holds base + query vectors plus their dimension.
#[derive(Debug, Clone)]
pub struct Corpus {
    pub kind: CorpusKind,
    pub dim: usize,
    pub base: Vec<Vector>,
    pub queries: Vec<Vector>,
    /// Optional ground-truth top-k per query, when shipped alongside
    /// the corpus (SIFT1M ships `sift_groundtruth.ivecs`).
    pub shipped_groundtruth: Option<Vec<Vec<u32>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "dimension mismatch in {path}: first record was {expected}, record {index} was {actual}"
    )]
    DimensionMismatch {
        path: PathBuf,
        expected: usize,
        actual: usize,
        index: usize,
    },
    #[error("dimension {dim} in {path} is outside the supported range 1..=4096")]
    DimensionOutOfRange { path: PathBuf, dim: usize },
    #[error("file {path} is empty")]
    Empty { path: PathBuf },
    #[error("token line {line} in {path} is malformed")]
    BadGloveLine { path: PathBuf, line: usize },
}

impl Corpus {
    /// Load a SIFT-style corpus given the three canonical files.
    ///
    /// `shipped_groundtruth` is optional — when provided we record it
    /// so the caller can sanity-check the brute-force result against
    /// the published ground truth.
    pub fn load_sift(
        base_fvecs: &Path,
        query_fvecs: &Path,
        groundtruth_ivecs: Option<&Path>,
    ) -> Result<Self, CorpusError> {
        let base = read_fvecs(base_fvecs)?;
        let queries = read_fvecs(query_fvecs)?;
        if base.is_empty() {
            return Err(CorpusError::Empty {
                path: base_fvecs.to_path_buf(),
            });
        }
        if queries.is_empty() {
            return Err(CorpusError::Empty {
                path: query_fvecs.to_path_buf(),
            });
        }
        let dim = base[0].len();
        if queries[0].len() != dim {
            return Err(CorpusError::DimensionMismatch {
                path: query_fvecs.to_path_buf(),
                expected: dim,
                actual: queries[0].len(),
                index: 0,
            });
        }
        let shipped_groundtruth = match groundtruth_ivecs {
            Some(p) => Some(read_ivecs(p)?),
            None => None,
        };
        Ok(Self {
            kind: CorpusKind::Sift,
            dim,
            base,
            queries,
            shipped_groundtruth,
        })
    }

    /// Load a GloVe-style corpus. The caller picks the first
    /// `query_count` rows as the query set and the rest as the base
    /// set — GloVe doesn't ship a separate query split.
    pub fn load_glove(
        glove_text: &Path,
        query_count: usize,
        base_limit: Option<usize>,
    ) -> Result<Self, CorpusError> {
        let mut all = read_glove(glove_text)?;
        if all.is_empty() {
            return Err(CorpusError::Empty {
                path: glove_text.to_path_buf(),
            });
        }
        if query_count >= all.len() {
            return Err(CorpusError::Empty {
                path: glove_text.to_path_buf(),
            });
        }
        let dim = all[0].len();
        let queries: Vec<Vector> = all.drain(..query_count).collect();
        if let Some(limit) = base_limit {
            all.truncate(limit);
        }
        Ok(Self {
            kind: CorpusKind::Glove,
            dim,
            base: all,
            queries,
            shipped_groundtruth: None,
        })
    }

    /// Construct a corpus directly from in-memory vectors. Intended
    /// for unit tests; production runs go through the file loaders.
    pub fn from_memory(dim: usize, base: Vec<Vector>, queries: Vec<Vector>) -> Self {
        Self {
            kind: CorpusKind::Synthetic,
            dim,
            base,
            queries,
            shipped_groundtruth: None,
        }
    }
}

/// Stream a `*.fvecs` file into a `Vec<Vec<f32>>`.
pub fn read_fvecs(path: &Path) -> Result<Vec<Vector>, CorpusError> {
    let file = File::open(path).map_err(|e| CorpusError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut reader = BufReader::new(file);
    let mut out = Vec::new();
    let mut expected_dim: Option<usize> = None;
    let mut idx = 0usize;
    loop {
        let dim_word = match reader.read_i32::<LittleEndian>() {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                return Err(CorpusError::Io {
                    path: path.to_path_buf(),
                    source: e,
                });
            }
        };
        if !(1..=4096).contains(&dim_word) {
            return Err(CorpusError::DimensionOutOfRange {
                path: path.to_path_buf(),
                dim: dim_word as usize,
            });
        }
        let dim = dim_word as usize;
        if let Some(prev) = expected_dim {
            if prev != dim {
                return Err(CorpusError::DimensionMismatch {
                    path: path.to_path_buf(),
                    expected: prev,
                    actual: dim,
                    index: idx,
                });
            }
        } else {
            expected_dim = Some(dim);
        }
        let mut vec = vec![0f32; dim];
        reader
            .read_f32_into::<LittleEndian>(&mut vec)
            .map_err(|e| CorpusError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
        out.push(vec);
        idx += 1;
    }
    Ok(out)
}

/// Stream a `*.ivecs` file (ground-truth IDs) into a `Vec<Vec<u32>>`.
pub fn read_ivecs(path: &Path) -> Result<Vec<Vec<u32>>, CorpusError> {
    let file = File::open(path).map_err(|e| CorpusError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut reader = BufReader::new(file);
    let mut out = Vec::new();
    let mut expected_dim: Option<usize> = None;
    let mut idx = 0usize;
    loop {
        let dim_word = match reader.read_i32::<LittleEndian>() {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                return Err(CorpusError::Io {
                    path: path.to_path_buf(),
                    source: e,
                });
            }
        };
        if !(1..=4096).contains(&dim_word) {
            return Err(CorpusError::DimensionOutOfRange {
                path: path.to_path_buf(),
                dim: dim_word as usize,
            });
        }
        let dim = dim_word as usize;
        if let Some(prev) = expected_dim {
            if prev != dim {
                return Err(CorpusError::DimensionMismatch {
                    path: path.to_path_buf(),
                    expected: prev,
                    actual: dim,
                    index: idx,
                });
            }
        } else {
            expected_dim = Some(dim);
        }
        let mut vec = vec![0u32; dim];
        for slot in vec.iter_mut() {
            *slot = reader
                .read_u32::<LittleEndian>()
                .map_err(|e| CorpusError::Io {
                    path: path.to_path_buf(),
                    source: e,
                })?;
        }
        out.push(vec);
        idx += 1;
    }
    Ok(out)
}

/// Parse a GloVe space-separated text file. The first whitespace-
/// delimited column is the token (discarded); the rest is the vector.
pub fn read_glove(path: &Path) -> Result<Vec<Vector>, CorpusError> {
    let file = File::open(path).map_err(|e| CorpusError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    let mut expected_dim: Option<usize> = None;
    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| CorpusError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let mut parts = line.split_whitespace();
        let _token = parts.next().ok_or_else(|| CorpusError::BadGloveLine {
            path: path.to_path_buf(),
            line: i,
        })?;
        let vec: Result<Vec<f32>, _> = parts.map(str::parse::<f32>).collect();
        let vec = vec.map_err(|_| CorpusError::BadGloveLine {
            path: path.to_path_buf(),
            line: i,
        })?;
        if vec.is_empty() {
            return Err(CorpusError::BadGloveLine {
                path: path.to_path_buf(),
                line: i,
            });
        }
        if let Some(prev) = expected_dim {
            if prev != vec.len() {
                return Err(CorpusError::DimensionMismatch {
                    path: path.to_path_buf(),
                    expected: prev,
                    actual: vec.len(),
                    index: i,
                });
            }
        } else {
            expected_dim = Some(vec.len());
        }
        out.push(vec);
    }
    Ok(out)
}

/// Write a `Vec<Vec<f32>>` to disk in `fvecs` format. Useful for
/// fixtures inside unit tests; not used by the CLI runtime.
#[doc(hidden)]
pub fn write_fvecs(path: &Path, vectors: &[Vector]) -> io::Result<()> {
    use std::io::Write;
    let mut file = File::create(path)?;
    let mut buf = Vec::new();
    for v in vectors {
        let dim = v.len() as i32;
        buf.extend_from_slice(&dim.to_le_bytes());
        for value in v {
            buf.extend_from_slice(&value.to_le_bytes());
        }
    }
    file.write_all(&buf)
}

/// Write `Vec<Vec<u32>>` in `ivecs` format. Same caveat as
/// [`write_fvecs`].
#[doc(hidden)]
pub fn write_ivecs(path: &Path, vectors: &[Vec<u32>]) -> io::Result<()> {
    use std::io::Write;
    let mut file = File::create(path)?;
    let mut buf = Vec::new();
    for v in vectors {
        let dim = v.len() as i32;
        buf.extend_from_slice(&dim.to_le_bytes());
        for value in v {
            buf.extend_from_slice(&value.to_le_bytes());
        }
    }
    file.write_all(&buf)
}

#[doc(hidden)]
pub fn _eat<R: Read>(_r: &mut R) {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn fvecs_roundtrip_preserves_dim_and_values() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("base.fvecs");
        let original: Vec<Vector> = vec![
            vec![1.0, 2.0, 3.0],
            vec![-0.5, 0.0, 7.25],
            vec![0.1, 0.2, 0.3],
        ];
        write_fvecs(&path, &original).expect("write");
        let parsed = read_fvecs(&path).expect("read");
        assert_eq!(parsed, original);
    }

    #[test]
    fn ivecs_roundtrip_preserves_ids() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("gt.ivecs");
        let original: Vec<Vec<u32>> = vec![vec![0, 1, 2, 3], vec![10, 20, 30, 40]];
        write_ivecs(&path, &original).expect("write");
        let parsed = read_ivecs(&path).expect("read");
        assert_eq!(parsed, original);
    }

    #[test]
    fn glove_text_parser_handles_tokens_and_floats() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("glove.txt");
        std::fs::write(&path, "the 0.1 0.2 0.3\nof -1.0 2.0 0.5\n").expect("seed glove text");
        let parsed = read_glove(&path).expect("read");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], vec![0.1, 0.2, 0.3]);
        assert_eq!(parsed[1], vec![-1.0, 2.0, 0.5]);
    }

    #[test]
    fn fvecs_rejects_dim_mismatch() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("mixed.fvecs");
        let bad: Vec<Vector> = vec![vec![1.0, 2.0], vec![1.0, 2.0, 3.0]];
        write_fvecs(&path, &bad).expect("write");
        let err = read_fvecs(&path).unwrap_err();
        assert!(matches!(err, CorpusError::DimensionMismatch { .. }));
    }

    #[test]
    fn corpus_load_sift_uses_provided_paths() {
        let dir = TempDir::new().expect("tempdir");
        let base = dir.path().join("base.fvecs");
        let queries = dir.path().join("queries.fvecs");
        let gt = dir.path().join("gt.ivecs");
        write_fvecs(
            &base,
            &[
                vec![1.0, 0.0, 0.0],
                vec![0.0, 1.0, 0.0],
                vec![0.0, 0.0, 1.0],
            ],
        )
        .expect("write base");
        write_fvecs(&queries, &[vec![1.0, 0.0, 0.0]]).expect("write queries");
        write_ivecs(&gt, &[vec![0, 1, 2]]).expect("write gt");
        let corpus = Corpus::load_sift(&base, &queries, Some(&gt)).expect("load");
        assert_eq!(corpus.kind, CorpusKind::Sift);
        assert_eq!(corpus.dim, 3);
        assert_eq!(corpus.base.len(), 3);
        assert_eq!(corpus.queries.len(), 1);
        assert_eq!(
            corpus.shipped_groundtruth.as_ref().unwrap()[0],
            vec![0, 1, 2]
        );
    }
}
