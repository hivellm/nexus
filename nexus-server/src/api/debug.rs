//! Runtime memory-profiling endpoints (feature = "memory-profiling").
//!
//! When the crate is compiled with `--features memory-profiling`, the global
//! allocator is switched to jemalloc with heap-profiling enabled. Ops can
//! then trigger on-demand heap dumps from the running process without
//! attaching a debugger. Dumps are written to the directory configured by
//! `MALLOC_CONF=prof_prefix=...` (defaults to the current working directory).
//!
//! Route summary:
//! - `GET  /debug/memory`       — current allocator stats (JSON).
//! - `POST /debug/heap/dump`    — trigger a heap profile dump to disk.
//!
//! Both routes return 503 if the server was built without the feature.

use axum::{Json, http::StatusCode};
use serde_json::{Value, json};

/// Human-readable allocator summary.
///
/// Reads jemalloc's `stats.*` keys after advancing the stats `epoch` so the
/// values reflect the current state rather than a stale snapshot.
pub async fn memory_stats() -> (StatusCode, Json<Value>) {
    #[cfg(all(feature = "memory-profiling", not(target_env = "msvc")))]
    {
        use tikv_jemalloc_ctl::{epoch, stats};

        // Advance the epoch so subsequent reads are fresh.
        if let Err(e) = epoch::advance() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "failed to advance jemalloc epoch",
                    "detail": e.to_string(),
                })),
            );
        }

        let allocated = stats::allocated::read().unwrap_or(0);
        let active = stats::active::read().unwrap_or(0);
        let metadata = stats::metadata::read().unwrap_or(0);
        let resident = stats::resident::read().unwrap_or(0);
        let mapped = stats::mapped::read().unwrap_or(0);
        let retained = stats::retained::read().unwrap_or(0);

        let body = json!({
            "allocator": "jemalloc",
            "feature": "memory-profiling",
            "bytes": {
                "allocated": allocated,
                "active": active,
                "metadata": metadata,
                "resident": resident,
                "mapped": mapped,
                "retained": retained,
            },
            "mib": {
                "allocated": allocated as f64 / 1_048_576.0,
                "active": active as f64 / 1_048_576.0,
                "resident": resident as f64 / 1_048_576.0,
                "mapped": mapped as f64 / 1_048_576.0,
            },
        });
        (StatusCode::OK, Json(body))
    }

    #[cfg(not(all(feature = "memory-profiling", not(target_env = "msvc"))))]
    {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "memory-profiling feature disabled",
                "hint": "rebuild nexus-server with --features memory-profiling",
            })),
        )
    }
}

/// Trigger a heap profile dump.
///
/// Writes a `.heap` file under `MALLOC_CONF.prof_prefix`. Post-process it
/// with `jeprof` (bundled with libjemalloc2) to produce pprof or call
/// graphs:
///
/// ```bash
/// jeprof --svg nexus-server heap.<pid>.0.f.heap > heap.svg
/// ```
pub async fn heap_dump() -> (StatusCode, Json<Value>) {
    #[cfg(all(feature = "memory-profiling", not(target_env = "msvc")))]
    {
        use tikv_jemalloc_ctl::raw::write;

        // mallctl("prof.dump", <c-string path or null for default prefix>)
        // SAFETY: the key is a NUL-terminated ASCII string and the value is
        // written as a raw C string argument that jemalloc copies out
        // before returning.
        let key = b"prof.dump\0";
        // Passing a null pointer tells jemalloc to use the configured
        // prof_prefix, which is the simplest and safest path.
        let value: *const std::ffi::c_char = std::ptr::null();
        let result = unsafe { write(key, value) };

        match result {
            Ok(()) => (
                StatusCode::OK,
                Json(json!({
                    "status": "dump_triggered",
                    "hint": "check the directory set by MALLOC_CONF.prof_prefix",
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "prof.dump mallctl failed — did you set MALLOC_CONF=prof:true?",
                    "detail": e.to_string(),
                })),
            ),
        }
    }

    #[cfg(not(all(feature = "memory-profiling", not(target_env = "msvc"))))]
    {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "memory-profiling feature disabled",
                "hint": "rebuild nexus-server with --features memory-profiling",
            })),
        )
    }
}
