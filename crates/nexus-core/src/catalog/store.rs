//! [`Catalog`] struct definition, LMDB environment management, and
//! constructor chain.
//!
//! This module owns the data layout of `Catalog` and every path that opens
//! or initialises the LMDB environment.  All business-logic `impl` blocks
//! live in sibling modules and are assembled via `#[path = "..."]` imports in
//! `mod.rs`.

use crate::catalog::external_id_index::ExternalIdIndex;
use crate::catalog::types::{CatalogMetadata, CatalogStats, KeyId, LabelId, TypeId};
use crate::{Error, Result};
use dashmap::DashMap;
use heed::types::*;
use heed::{Database, Env, EnvOpenOptions, byteorder};
use parking_lot::RwLock;
use std::path::Path;
use std::sync::Arc;

/// Default LMDB `map_size` for a catalog environment — 100 MiB.
///
/// Sized for the catalog's workload: label / type / key name strings
/// plus their u32 ID mappings, metadata, statistics, constraints,
/// UDFs, and procedures. Even a production deployment with tens of
/// thousands of labels comfortably fits under this ceiling. LMDB
/// reserves virtual address space up to `map_size` eagerly on
/// `Env::open`, so picking this too large wastes address space on
/// Windows where TLS-slot pressure grows with the number of opened
/// environments; picking it too small surfaces as `MDB_MAP_FULL`
/// under catalog churn. 100 MiB is the working compromise measured
/// during the phase4 magic-constant audit.
///
/// Callers that need a larger map explicitly pass their own value to
/// [`Catalog::with_map_size`] / [`Catalog::with_isolated_path`].
pub const CATALOG_MMAP_INITIAL_SIZE: usize = 100 * 1024 * 1024;

/// Catalog for managing label/type/key mappings.
///
/// Thread-safe via `RwLock` for concurrent reads.
#[derive(Clone)]
pub struct Catalog {
    /// LMDB environment.
    ///
    /// `pub(crate)` so that `#[cfg(test)]` code in sibling modules (e.g.
    /// `catalog::tests`) can open raw transactions to test the
    /// external-id index without going through the public `write_txn` /
    /// `read_txn` helpers.  No non-test code outside this module should
    /// access this field directly.
    pub(crate) env: Arc<Env>,

    /// Label name → ID mapping.
    pub(super) label_name_to_id: Database<Str, U32<byteorder::NativeEndian>>,
    /// Label ID → name mapping.
    pub(super) label_id_to_name: Database<U32<byteorder::NativeEndian>, Str>,

    /// Type name → ID mapping.
    pub(super) type_name_to_id: Database<Str, U32<byteorder::NativeEndian>>,
    /// Type ID → name mapping.
    pub(super) type_id_to_name: Database<U32<byteorder::NativeEndian>, Str>,

    /// Key name → ID mapping.
    pub(super) key_name_to_id: Database<Str, U32<byteorder::NativeEndian>>,
    /// Key ID → name mapping.
    pub(super) key_id_to_name: Database<U32<byteorder::NativeEndian>, Str>,

    /// Metadata database (version, epoch, config).
    pub(super) metadata_db: Database<Str, SerdeBincode<CatalogMetadata>>,

    /// Statistics database.
    pub(super) stats_db: Database<Str, SerdeBincode<CatalogStats>>,

    /// Constraint manager.
    pub(super) constraint_manager: Arc<RwLock<crate::catalog::constraints::ConstraintManager>>,

    /// UDF storage database (name → signature).
    pub(super) udf_db: Database<Str, SerdeBincode<crate::udf::UdfSignature>>,

    /// Procedure storage database (name → signature).
    pub(super) procedure_db:
        Database<Str, SerdeBincode<crate::graph::procedures::ProcedureSignature>>,

    /// Durable property-index definitions: the set of `(label_id, key_id)`
    /// pairs registered by `CREATE INDEX`. Reloaded at startup to rebuild
    /// the typed property index so indexes survive a restart (issue #11).
    pub(super) property_index_db: Database<SerdeBincode<(u32, u32)>, SerdeBincode<()>>,

    /// Next label ID counter (cached for performance).
    pub(super) next_label_id: Arc<RwLock<u32>>,
    /// Next type ID counter.
    pub(super) next_type_id: Arc<RwLock<u32>>,
    /// Next key ID counter.
    pub(super) next_key_id: Arc<RwLock<u32>>,

    /// In-memory cache for label name → ID lookups (lock-free).
    pub(super) label_name_cache: Arc<DashMap<String, u32>>,
    /// In-memory cache for label ID → name lookups (lock-free).
    pub(super) label_id_cache: Arc<DashMap<u32, String>>,
    /// In-memory cache for type name → ID lookups (lock-free).
    pub(super) type_name_cache: Arc<DashMap<String, u32>>,
    /// In-memory cache for type ID → name lookups (lock-free).
    pub(super) type_id_cache: Arc<DashMap<u32, String>>,
    /// In-memory cache for key name → ID lookups (lock-free).
    pub(super) key_name_cache: Arc<DashMap<String, u32>>,
    /// In-memory cache for key ID → name lookups (lock-free).
    pub(super) key_id_cache: Arc<DashMap<u32, String>>,

    /// External node id index (forward + reverse LMDB sub-databases).
    pub(super) external_id_index: Arc<ExternalIdIndex>,
}

impl Catalog {
    /// Create a new catalog instance.
    ///
    /// Opens or creates the LMDB environment at `path`. Under `cargo
    /// test`, `Catalog::with_map_size` transparently redirects to a
    /// shared `nexus_test_catalogs_shared` directory under
    /// `std::env::temp_dir()` so the whole test suite lives in a
    /// single LMDB environment — `TestContext` callers that want a
    /// fresh environment should use [`Catalog::with_isolated_path`]
    /// instead.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for LMDB files
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nexus_core::catalog::Catalog;
    ///
    /// let catalog = Catalog::new("./data/catalog").unwrap();
    /// ```
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        // Test runs pick a smaller map size so the shared LMDB
        // environment does not reserve gigabytes of address space.
        let is_test = std::env::var("CARGO_PKG_NAME").is_ok()
            || std::env::var("CARGO").is_ok()
            || std::env::args().any(|arg| arg.contains("test") || arg.contains("cargo"));
        let map_size = if is_test { 512 * 1024 } else { 1024 * 1024 };

        Self::with_map_size(path, map_size)
    }

    /// Create a new catalog with a specific map_size.
    ///
    /// This is useful for testing or when you need to control the LMDB map
    /// size.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for LMDB files
    /// * `map_size` - Maximum size of the LMDB memory map in bytes
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nexus_core::catalog::Catalog;
    ///
    /// // Create catalog with 100MB map size for testing
    /// let catalog = Catalog::with_map_size(
    ///     "./data/catalog",
    ///     nexus_core::catalog::CATALOG_MMAP_INITIAL_SIZE,
    /// ).unwrap();
    /// ```
    pub fn with_map_size<P: AsRef<Path>>(path: P, map_size: usize) -> Result<Self> {
        use std::sync::OnceLock;

        // In test mode, use a shared directory pool to reduce number of LMDB
        // environments.  This prevents TlsFull errors when many tests run in
        // parallel.
        let is_test = std::env::var("CARGO_PKG_NAME").is_ok()
            || std::env::var("CARGO").is_ok()
            || std::env::args().any(|arg| arg.contains("test") || arg.contains("cargo"));

        // In test mode, use a fixed map_size to avoid BadOpenOptions errors
        // when multiple tests try to open the same environment with different
        // options.
        let actual_map_size = if is_test {
            // Use a fixed map_size for all tests to allow sharing environments.
            CATALOG_MMAP_INITIAL_SIZE
        } else {
            map_size
        };

        let actual_path = if is_test {
            // Use a SINGLE shared test directory for ALL catalogs in tests.
            // This prevents TlsFull errors on Windows by limiting to just 1
            // LMDB environment.
            static TEST_CATALOG_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

            let shared_dir = TEST_CATALOG_DIR.get_or_init(|| {
                // One shared LMDB directory PER PROCESS (keyed by pid).
                // `get_or_init` runs once per process, so wiping the dir
                // here resets every label / type / key id to zero at the
                // start of each `cargo test` run — without the wipe, LMDB
                // state persists across runs and tests that assert on
                // `db.labels()` content see accumulated cruft that
                // eventually causes `get_or_create_label` to allocate ids
                // past the 64-bit `label_bits` cap (and silently drop
                // newly-registered labels).
                //
                // CRITICAL: the directory MUST be process-scoped. A single
                // `cargo test -p nexus-core` invocation launches MANY test
                // binaries (the lib binary plus one per integration file)
                // as separate processes. If they all share ONE fixed dir,
                // each process's wipe (`remove_dir_all`) + concurrent LMDB
                // writes corrupt the others' catalog mid-run — label ids
                // get reset/reassigned, so a query's label resolution no
                // longer matches the `label_bits` written at CREATE time,
                // and label-scoped filters collapse. That was the root
                // cause of the load-dependent `match_scopes_*` flake. The
                // pid suffix keeps exactly one LMDB environment per process
                // (still avoids the Windows TlsFull error) while giving each
                // concurrent test binary its own isolated catalog.
                let dir = std::env::temp_dir()
                    .join(format!("nexus_test_catalogs_shared_{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).ok();
                dir
            });

            shared_dir.clone()
        } else {
            path.as_ref().to_path_buf()
        };

        Self::open_at_path(&actual_path, actual_map_size)
    }

    /// Create a catalog with an isolated path (bypasses test sharing).
    ///
    /// WARNING: Use sparingly! Each call creates a new LMDB environment.
    /// Only use for tests that absolutely require data isolation.
    /// This is available for both unit tests and integration tests.
    pub fn with_isolated_path<P: AsRef<Path>>(path: P, map_size: usize) -> Result<Self> {
        Self::open_at_path(path.as_ref(), map_size)
    }

    /// Internal function to open catalog at a specific path.
    pub(super) fn open_at_path(actual_path: &Path, actual_map_size: usize) -> Result<Self> {
        // Create directory if it doesn't exist.
        std::fs::create_dir_all(actual_path)?;

        // Open LMDB environment with specified map size, 15 databases.
        // `max_readers` is bumped from LMDB's 126 default because the
        // test binary holds a single shared catalog env across ~2000
        // parallel tests, each opening at least one read txn per
        // query. Without the bump, the env exhausts TLS reader slots
        // and subsequent `env.read_txn()` / `env.write_txn()` calls
        // return `Database(Mdb(TlsFull))` — surfaced as flaky failures
        // in `graph::core::tests::test_edge_is_empty` and any other
        // test that happens to try to open a (read) txn while the
        // slot table is full. 2048 slots covers the maximum parallel
        // depth `cargo test` uses at the default (logical-core) thread
        // count on a typical 16-core bench box.
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(actual_map_size)
                .max_dbs(17) // Increased for constraints, UDFs, procedures, and external-id databases
                .max_readers(2048)
                .open(actual_path)?
        };
        let env = Arc::new(env);

        // Open/create databases.
        let mut wtxn = env.write_txn()?;

        let label_name_to_id = env.create_database(&mut wtxn, Some("label_name_to_id"))?;
        let label_id_to_name = env.create_database(&mut wtxn, Some("label_id_to_name"))?;

        let type_name_to_id = env.create_database(&mut wtxn, Some("type_name_to_id"))?;
        let type_id_to_name = env.create_database(&mut wtxn, Some("type_id_to_name"))?;

        let key_name_to_id = env.create_database(&mut wtxn, Some("key_name_to_id"))?;
        let key_id_to_name = env.create_database(&mut wtxn, Some("key_id_to_name"))?;

        let metadata_db = env.create_database(&mut wtxn, Some("metadata"))?;
        let stats_db = env.create_database(&mut wtxn, Some("statistics"))?;

        // Create constraint databases.
        let constraints_db: Database<
            SerdeBincode<(u32, u32)>,
            SerdeBincode<crate::catalog::constraints::Constraint>,
        > = env.create_database(&mut wtxn, Some("constraints"))?;
        let constraint_id_to_key: Database<U32<byteorder::NativeEndian>, SerdeBincode<(u32, u32)>> =
            env.create_database(&mut wtxn, Some("constraint_id_to_key"))?;

        // Create UDF storage database (name → signature).
        let udf_db: Database<Str, SerdeBincode<crate::udf::UdfSignature>> =
            env.create_database(&mut wtxn, Some("udfs"))?;

        // Create procedure storage database (name → signature).
        let procedure_db: Database<
            Str,
            SerdeBincode<crate::graph::procedures::ProcedureSignature>,
        > = env.create_database(&mut wtxn, Some("procedures"))?;

        // Create the durable property-index definition store (issue #11).
        let property_index_db: Database<SerdeBincode<(u32, u32)>, SerdeBincode<()>> =
            env.create_database(&mut wtxn, Some("property_indexes"))?;

        // Create external-id index sub-databases (forward + reverse).
        let external_id_index = ExternalIdIndex::open(&env, &mut wtxn)?;

        // Initialize metadata if not exists.
        if metadata_db.get(&wtxn, "main")?.is_none() {
            let metadata = CatalogMetadata::default();
            metadata_db.put(&mut wtxn, "main", &metadata)?;
        }

        // Initialize statistics if not exists.
        if stats_db.get(&wtxn, "main")?.is_none() {
            let stats = CatalogStats::default();
            stats_db.put(&mut wtxn, "main", &stats)?;
        }

        wtxn.commit()?;

        // Initialize counters by scanning existing data.
        let rtxn = env.read_txn()?;

        let next_label_id = label_name_to_id
            .iter(&rtxn)?
            .map(|r| r.map(|(_, id)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|max_id| max_id + 1)
            .unwrap_or(0);

        let next_type_id = type_name_to_id
            .iter(&rtxn)?
            .map(|r| r.map(|(_, id)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|max_id| max_id + 1)
            .unwrap_or(0);

        let next_key_id = key_name_to_id
            .iter(&rtxn)?
            .map(|r| r.map(|(_, id)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|max_id| max_id + 1)
            .unwrap_or(0);

        // Drop transaction before moving env.
        drop(rtxn);

        // Initialize in-memory caches from LMDB.
        let label_name_cache = Arc::new(DashMap::new());
        let label_id_cache = Arc::new(DashMap::new());
        let type_name_cache = Arc::new(DashMap::new());
        let type_id_cache = Arc::new(DashMap::new());
        let key_name_cache = Arc::new(DashMap::new());
        let key_id_cache = Arc::new(DashMap::new());

        // Warm up caches from existing data.
        // Populate caches immediately to ensure consistency.
        {
            let rtxn = env.read_txn()?;
            for result in label_name_to_id.iter(&rtxn)? {
                if let Ok((name, id)) = result {
                    let name_str: &str = name;
                    label_name_cache.insert(name_str.to_string(), id);
                    label_id_cache.insert(id, name_str.to_string());
                }
            }
            for result in type_name_to_id.iter(&rtxn)? {
                if let Ok((name, id)) = result {
                    let name_str: &str = name;
                    type_name_cache.insert(name_str.to_string(), id);
                    type_id_cache.insert(id, name_str.to_string());
                }
            }
            for result in key_name_to_id.iter(&rtxn)? {
                if let Ok((name, id)) = result {
                    let name_str: &str = name;
                    key_name_cache.insert(name_str.to_string(), id);
                    key_id_cache.insert(id, name_str.to_string());
                }
            }
        }

        // Initialize constraint manager with existing databases.
        let constraint_manager =
            crate::catalog::constraints::ConstraintManager::new_with_databases(
                env.as_ref(),
                constraints_db,
                constraint_id_to_key,
            )?;

        Ok(Self {
            env,
            label_name_to_id,
            label_id_to_name,
            type_name_to_id,
            type_id_to_name,
            key_name_to_id,
            key_id_to_name,
            metadata_db,
            stats_db,
            constraint_manager: Arc::new(RwLock::new(constraint_manager)),
            udf_db,
            procedure_db,
            property_index_db,
            next_label_id: Arc::new(RwLock::new(next_label_id)),
            next_type_id: Arc::new(RwLock::new(next_type_id)),
            next_key_id: Arc::new(RwLock::new(next_key_id)),
            label_name_cache,
            label_id_cache,
            type_name_cache,
            type_id_cache,
            key_name_cache,
            key_id_cache,
            external_id_index: Arc::new(external_id_index),
        })
    }

    /// Sync environment to disk (fsync).
    pub fn sync(&self) -> Result<()> {
        self.env.force_sync()?;
        Ok(())
    }

    /// Health check for the catalog.
    pub fn health_check(&self) -> Result<()> {
        // Try to read from the catalog to verify it's accessible.
        let rtxn = self.env.read_txn()?;

        // Check if we can read from all databases.
        let _ = self.label_name_to_id.len(&rtxn)?;
        let _ = self.label_id_to_name.len(&rtxn)?;
        let _ = self.type_name_to_id.len(&rtxn)?;
        let _ = self.type_id_to_name.len(&rtxn)?;
        let _ = self.key_name_to_id.len(&rtxn)?;
        let _ = self.key_id_to_name.len(&rtxn)?;
        let _ = self.metadata_db.len(&rtxn)?;
        let _ = self.stats_db.len(&rtxn)?;

        drop(rtxn);
        Ok(())
    }

    /// Get the number of labels.
    pub fn label_count(&self) -> u64 {
        let next_id = self.next_label_id.read();
        *next_id as u64
    }

    /// Get the number of relationship types.
    pub fn rel_type_count(&self) -> u64 {
        let next_id = self.next_type_id.read();
        *next_id as u64
    }

    /// Open a write transaction on the catalog LMDB environment.
    ///
    /// Callers that need to write external-id index entries in the same
    /// LMDB transaction as other catalog mutations should use this to
    /// obtain an `RwTxn` and then commit it when done.
    pub fn write_txn(&self) -> Result<heed::RwTxn<'_>> {
        Ok(self.env.write_txn()?)
    }

    /// Open a read transaction on the catalog LMDB environment.
    pub fn read_txn(&self) -> Result<heed::RoTxn<'_>> {
        Ok(self.env.read_txn()?)
    }
}

impl Default for Catalog {
    /// Build a fresh `Catalog` backed by a throwaway directory.
    ///
    /// Previously this returned a clone of a process-wide
    /// `SHARED_CATALOG` rooted at `./data/catalog` (a path relative
    /// to the current working directory). Under `cargo test` with the
    /// default parallelism that meant every test was hammering the
    /// same catalog in the project root, and the first test to create
    /// a label permanently polluted every subsequent test's label-id
    /// enumeration. It also left stray `./data/catalog/*.mdb` files
    /// behind every test run.
    ///
    /// Post `phase3_remove-test-shared-state` the default impl uses
    /// `tempfile::tempdir().keep()` for the root path and calls
    /// `Catalog::new` — which under `cargo test` still gets folded
    /// into the per-process `nexus_test_catalogs_shared` directory
    /// via `Catalog::with_map_size`, so file-descriptor usage stays
    /// bounded, but the relative-path pollution of the project tree
    /// is gone. Tests that need strict catalog isolation (fresh
    /// label/type IDs) should call
    /// [`Catalog::with_isolated_path`] directly instead of going
    /// through `default`.
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create default-catalog temp dir");
        let path = temp_dir.keep();
        Self::new(&path).expect("Failed to create default catalog")
    }
}
