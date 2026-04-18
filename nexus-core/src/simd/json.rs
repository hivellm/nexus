//! Size-threshold JSON parsing: `simd-json` above the threshold,
//! `serde_json` below it.
//!
//! `simd-json` mutates its input (stage-1 UTF-8 validation rewrites
//! the buffer in place) and is measurably faster than `serde_json`
//! for payloads ≥ ~64 KiB on SSE4.2/AVX2. Below that size the
//! per-call fixed overhead of `simd-json`'s state tape outweighs the
//! throughput advantage and `serde_json` wins — plus we avoid the
//! `.to_vec()` allocation an immutable body would otherwise pay.
//!
//! The dispatch threshold is tuned for Nexus workloads: an ingest
//! payload of 10 000 small nodes sits around 100 KiB (well into the
//! simd-json regime) while a typical Cypher `parameters` map under
//! 1 KiB stays on serde_json.
//!
//! The escape hatch (`NEXUS_SIMD_JSON_DISABLE=1`) forces serde_json
//! for every call and is the runtime rollback lever if a simd-json
//! edge case surfaces in production.

use serde::de::DeserializeOwned;
use std::sync::OnceLock;

/// Size threshold (bytes) above which `simd-json` is used. Chosen so
/// the buffer clone (`Vec<u8>`) simd-json needs is amortised across a
/// measurable parse-time win. Exposed for tests; not part of the
/// public API surface.
pub const SIMD_JSON_THRESHOLD_BYTES: usize = 64 * 1024;

fn simd_json_disabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| {
        std::env::var_os("NEXUS_SIMD_JSON_DISABLE")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    })
}

/// Parse `body` into `T`, routing to simd-json when the buffer is
/// large enough and the env override is not set. Falls through to
/// serde_json otherwise.
///
/// This is a one-shot parser: callers hand over an immutable slice
/// and accept the `Vec<u8>` clone on the simd-json branch. The clone
/// avoids changing every caller to take `&mut [u8]` and keeps the
/// scalar (serde_json) branch allocation-free.
pub fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T, JsonError> {
    if body.len() >= SIMD_JSON_THRESHOLD_BYTES && !simd_json_disabled() {
        let mut owned = body.to_vec();
        simd_json::serde::from_slice(&mut owned).map_err(JsonError::SimdJson)
    } else {
        serde_json::from_slice(body).map_err(JsonError::SerdeJson)
    }
}

/// Same as [`parse`] but the caller supplies a mutable buffer that
/// simd-json can overwrite directly — useful when the caller already
/// owns a `Vec<u8>` (e.g. axum `Bytes::to_vec()` or the RPC binary
/// frame buffer), avoiding the extra clone.
pub fn parse_mut<T: DeserializeOwned>(body: &mut Vec<u8>) -> Result<T, JsonError> {
    if body.len() >= SIMD_JSON_THRESHOLD_BYTES && !simd_json_disabled() {
        simd_json::serde::from_slice(body.as_mut_slice()).map_err(JsonError::SimdJson)
    } else {
        serde_json::from_slice(body.as_slice()).map_err(JsonError::SerdeJson)
    }
}

/// Which parser served a given call — exported for `/stats` and
/// test observability.
pub fn parser_for_len(len: usize) -> &'static str {
    if simd_json_disabled() {
        "serde_json (NEXUS_SIMD_JSON_DISABLE)"
    } else if len >= SIMD_JSON_THRESHOLD_BYTES {
        "simd-json"
    } else {
        "serde_json"
    }
}

/// Errors from the size-dispatched JSON parser.
///
/// Wraps both underlying parsers so callers can build a single
/// error-handling path without caring which one served the call.
#[derive(Debug, thiserror::Error)]
pub enum JsonError {
    #[error("simd-json: {0}")]
    SimdJson(#[from] simd_json::Error),
    #[error("serde_json: {0}")]
    SerdeJson(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Row {
        id: u64,
        name: String,
        score: f64,
        tags: Vec<String>,
    }

    fn sample_rows(n: usize) -> Vec<Row> {
        (0..n)
            .map(|i| Row {
                id: i as u64,
                name: format!("row-{i}"),
                score: (i as f64) * 0.5,
                tags: vec!["a".into(), "b".into(), format!("t{}", i % 7)],
            })
            .collect()
    }

    #[test]
    fn small_body_uses_serde_json() {
        let rows = sample_rows(3);
        let body = serde_json::to_vec(&rows).unwrap();
        assert!(body.len() < SIMD_JSON_THRESHOLD_BYTES);
        assert_eq!(parser_for_len(body.len()), "serde_json");
        let parsed: Vec<Row> = parse(&body).unwrap();
        assert_eq!(parsed, rows);
    }

    #[test]
    fn large_body_uses_simd_json() {
        // ~ 100 KiB of rows
        let rows = sample_rows(2_000);
        let body = serde_json::to_vec(&rows).unwrap();
        assert!(body.len() >= SIMD_JSON_THRESHOLD_BYTES);
        assert_eq!(parser_for_len(body.len()), "simd-json");
        let parsed: Vec<Row> = parse(&body).unwrap();
        assert_eq!(parsed, rows);
    }

    #[test]
    fn parse_mut_avoids_clone_path() {
        let rows = sample_rows(2_000);
        let mut body = serde_json::to_vec(&rows).unwrap();
        let parsed: Vec<Row> = parse_mut(&mut body).unwrap();
        assert_eq!(parsed, rows);
    }

    #[test]
    fn invalid_json_surfaces_error() {
        let err = parse::<Row>(b"{not json").unwrap_err();
        assert!(!format!("{err}").is_empty());
    }
}
