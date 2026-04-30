//! Query-plan cache primitives.
//!
//! # Scope
//!
//! Phase 8 ships the **canonicaliser** that turns a Cypher query
//! string into a cache-key-friendly form. Two queries that differ
//! only in whitespace, comments, or trailing punctuation
//! canonicalise to the same string and therefore hit the same
//! plan cache entry.
//!
//! The full plan-cache integration (lookup at `Engine::execute`,
//! schema-change invalidation, `db.planCache.*` procedures, env
//! vars, `/stats` counters, Prometheus metrics) is tracked under
//! `phase8_query-plan-cache`; the canonicaliser ships first so the
//! existing per-optimizer plan cache in
//! `crates/nexus-core/src/executor/optimizer.rs` can adopt it
//! without waiting on the rest. The contract here is stable; a
//! future cache wiring consumes it without changes.
//!
//! # Why not just hash raw text
//!
//! `MATCH (n) RETURN n` and `MATCH (n)  RETURN n` (extra space)
//! hash to different cache entries today, missing every cache
//! lookup that should have hit. Comments (`MATCH (n) // comment`)
//! also miss. Real workloads paste the same parameterised query
//! through templating that injects insignificant whitespace and
//! comments; canonicalising before hashing tightens the hit rate
//! without changing semantics.
//!
//! # What the canonicaliser does
//!
//! 1. Strips line comments (`// ...` to end of line).
//! 2. Strips block comments (`/* ... */`, non-nested).
//! 3. Collapses every run of ASCII whitespace (space, tab,
//!    newline, carriage return) to a single space.
//! 4. Trims leading + trailing whitespace.
//! 5. Preserves string literals byte-for-byte — the contents of
//!    `"..."` and `'...'` are not touched, so a query like
//!    `MATCH (n {note: 'a  b'})` keeps its literal intact.
//!
//! # What the canonicaliser does NOT do
//!
//! - Does not lower-case keywords. `MATCH` and `match` produce
//!   different canonical strings — by design. Cypher is case-
//!   sensitive on identifiers and the parser already accepts
//!   case-insensitive keywords; lower-casing here would risk
//!   false aliasing of identifiers like `match` (a property
//!   name) with the keyword.
//! - Does not normalise parameter placeholders. `$x` and `$y`
//!   produce different keys. The plan-cache key is the
//!   *parameterised* query text; values are not part of the
//!   key, but parameter names are because they participate in
//!   binding. Two queries with different `$` names produce
//!   different plans (different variable scopes).
//! - Does not parse the query. The canonicaliser is character-
//!   level so it stays cheap (sub-microsecond on typical
//!   queries) and cannot fail on bad syntax — the plan cache
//!   miss path then runs the real parser, which does fail.

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;

/// Cypher comment + whitespace canonicaliser. Returns a
/// `Cow::Borrowed` when the input is already canonical (saves an
/// allocation on every cache hit), otherwise an owned `String`.
///
/// Stable across releases — bumping the canonicaliser bumps the
/// effective cache key, so the version is pinned via the
/// [`CANONICAL_VERSION`] constant. A future shape change requires
/// the constant to advance so callers that include it in their
/// hash invalidate their caches.
pub fn canonicalise_query(input: &str) -> Cow<'_, str> {
    // Fast path: scan for any non-canonical char (comments,
    // multi-space runs, leading/trailing whitespace). If none
    // present, return the input borrowed.
    if is_already_canonical(input) {
        return Cow::Borrowed(input);
    }
    Cow::Owned(rewrite(input))
}

/// Version stamp for the canonical form. Append to a hash key when
/// the canonical form participates in long-lived persistence (a
/// cache that survives restarts, a key in a regression report).
/// The current implementation produces version 1.
pub const CANONICAL_VERSION: u32 = 1;

fn is_already_canonical(input: &str) -> bool {
    if input.is_empty() {
        return true;
    }
    let bytes = input.as_bytes();
    if bytes.first().is_some_and(|b| b.is_ascii_whitespace())
        || bytes.last().is_some_and(|b| b.is_ascii_whitespace())
    {
        return false;
    }
    let mut prev_space = false;
    let mut iter = input.char_indices().peekable();
    while let Some((_, ch)) = iter.next() {
        match ch {
            // Any embedded comment marker disqualifies (we do not
            // distinguish "comment inside a string" here — fast
            // path falls through to the rewriter, which does).
            '/' => {
                if let Some((_, next)) = iter.peek() {
                    if *next == '/' || *next == '*' {
                        return false;
                    }
                }
            }
            '\'' | '"' => {
                // Skip the literal — we don't care what's inside.
                let quote = ch;
                while let Some((_, next)) = iter.next() {
                    if next == '\\' {
                        // Skip the escaped char.
                        iter.next();
                        continue;
                    }
                    if next == quote {
                        break;
                    }
                }
                prev_space = false;
            }
            c if c == '\t' || c == '\n' || c == '\r' => return false,
            ' ' => {
                if prev_space {
                    return false;
                }
                prev_space = true;
            }
            _ => prev_space = false,
        }
    }
    true
}

fn rewrite(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut bytes = input.bytes().enumerate();
    let raw = input.as_bytes();
    let mut prev_space = true; // suppress leading whitespace

    while let Some((i, b)) = bytes.next() {
        match b {
            b'/' if raw.get(i + 1) == Some(&b'/') => {
                // Line comment: skip to end of line (or EOF).
                for (_, c) in bytes.by_ref() {
                    if c == b'\n' {
                        break;
                    }
                }
                if !prev_space {
                    out.push(' ');
                    prev_space = true;
                }
            }
            b'/' if raw.get(i + 1) == Some(&b'*') => {
                // Block comment: skip until closing `*/` (non-nested).
                bytes.next(); // consume `*`
                let mut last = 0u8;
                for (_, c) in bytes.by_ref() {
                    if last == b'*' && c == b'/' {
                        break;
                    }
                    last = c;
                }
                if !prev_space {
                    out.push(' ');
                    prev_space = true;
                }
            }
            b'\'' | b'"' => {
                let quote = b;
                out.push(quote as char);
                // Copy the literal verbatim, honouring `\` escapes.
                let mut escaped = false;
                for (_, c) in bytes.by_ref() {
                    out.push(c as char);
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if c == b'\\' {
                        escaped = true;
                        continue;
                    }
                    if c == quote {
                        break;
                    }
                }
                prev_space = false;
            }
            b' ' | b'\t' | b'\n' | b'\r' => {
                if !prev_space {
                    out.push(' ');
                    prev_space = true;
                }
            }
            _ => {
                out.push(b as char);
                prev_space = false;
            }
        }
    }
    // Trim trailing space if we emitted one.
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Hash a query for plan-cache lookup using the canonicalised form
/// + the canonical version stamp. Uses `xxhash_rust::xxh3::xxh3_64`
/// — the same hasher already in nexus-core's hot paths.
///
/// Stable: same query, same canonical version → same hash.
pub fn hash_canonicalised(input: &str) -> u64 {
    use xxhash_rust::xxh3::Xxh3;
    let canonical = canonicalise_query(input);
    let mut hasher = Xxh3::new();
    hasher.update(&CANONICAL_VERSION.to_le_bytes());
    hasher.update(canonical.as_bytes());
    hasher.digest()
}

// ---------------------------------------------------------------------------
// PlanCache — process-wide LRU plan cache with generation invalidation.
// ---------------------------------------------------------------------------

/// Default capacity when neither `NEXUS_PLAN_CACHE_ENTRIES` nor the
/// programmatic constructor specifies one. Picked to keep the cache
/// at ~1 MiB for the common `OptimizationResult` shape (~1 KiB each).
pub const DEFAULT_PLAN_CACHE_ENTRIES: usize = 1024;

/// One entry stored in the [`PlanCache`].
#[derive(Debug, Clone)]
pub struct CachedEntry<V> {
    /// The cached value (typically `OptimizationResult` or
    /// `ExecutionPlan`). Cloned on every hit; callers wrap heavy
    /// payloads in `Arc<...>` if cloning is non-trivial.
    pub value: V,
    /// Snapshot of `PlanCache::generation()` at populate time. The
    /// lookup path compares against the current generation and
    /// evicts the entry when they differ — that is the
    /// schema-change invalidation gate.
    pub generation: u64,
    /// Total successful lookups this entry has served. Used by
    /// `db.planCache.list(top_n)` to surface hot plans.
    pub access_count: u64,
}

/// Stats snapshot returned by [`PlanCache::stats`]. All counters
/// are monotonic since process start — `db.planCache.clear()`
/// resets the cache contents but does not reset the counters
/// (operators want to see total hit / miss volume across the
/// process lifetime, not just since the last flush).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct PlanCacheStats {
    /// Number of successful lookups since process start.
    pub hits: u64,
    /// Number of lookups that missed (no entry, or stale generation).
    pub misses: u64,
    /// Number of entries evicted by the LRU bound or by generation
    /// drift. Includes `clear()`-time bulk eviction.
    pub evictions: u64,
    /// Current number of entries in the cache.
    pub size: usize,
    /// Maximum number of entries the cache will hold before LRU
    /// eviction kicks in.
    pub capacity: usize,
    /// `false` when the cache is disabled via `NEXUS_PLAN_CACHE_DISABLE`
    /// or the programmatic constructor — every `lookup` returns
    /// `None` and `insert` is a no-op.
    pub enabled: bool,
    /// Current planner generation. Incremented on schema changes
    /// (CREATE / DROP INDEX, CREATE / DROP CONSTRAINT, label / type
    /// / key registry changes). Cached entries with a different
    /// generation surface as misses.
    pub generation: u64,
}

/// Process-wide query-plan cache.
///
/// Generic over the cached value `V` so callers can stash whatever
/// fits their layer:
///
/// - Optimizer-level: `OptimizationResult`.
/// - Engine-level: `Arc<ExecutionPlan>`.
///
/// Both paths share the same canonicaliser
/// ([`hash_canonicalised`]) so a query templated through different
/// callers still hits the same entry.
///
/// Concurrency: a single `parking_lot::Mutex` guards the LRU map +
/// recency queue. Lookup is `O(1)` plus the lock acquisition;
/// `parking_lot::Mutex` is uncontended-fast and has no poisoning
/// to recover from. The atomic counters are read without the
/// lock.
pub struct PlanCache<V> {
    inner: Mutex<PlanCacheInner<V>>,
    capacity: usize,
    enabled: bool,
    generation: AtomicU64,
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
}

struct PlanCacheInner<V> {
    map: HashMap<u64, CachedEntry<V>>,
    order: VecDeque<u64>,
}

impl<V: Clone> PlanCache<V> {
    /// Build a cache with the given capacity. `capacity = 0`
    /// short-circuits every operation to the disabled path.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let enabled = capacity > 0;
        Self {
            inner: Mutex::new(PlanCacheInner {
                map: HashMap::with_capacity(capacity.max(1)),
                order: VecDeque::with_capacity(capacity.max(1)),
            }),
            capacity,
            enabled,
            generation: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    /// Build a cache from environment knobs:
    ///
    /// * `NEXUS_PLAN_CACHE_DISABLE` — when set to `1` / `true` /
    ///   `yes`, returns a fully disabled cache regardless of the
    ///   entries knob. Useful for ops emergencies and debugging.
    /// * `NEXUS_PLAN_CACHE_ENTRIES` — capacity. Defaults to
    ///   [`DEFAULT_PLAN_CACHE_ENTRIES`] when unset or unparseable.
    #[must_use]
    pub fn from_env() -> Self {
        let disabled = std::env::var("NEXUS_PLAN_CACHE_DISABLE")
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
            .unwrap_or(false);
        if disabled {
            return Self::disabled();
        }
        let capacity = std::env::var("NEXUS_PLAN_CACHE_ENTRIES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_PLAN_CACHE_ENTRIES);
        Self::new(capacity)
    }

    /// Build a permanently disabled cache. Every `lookup` returns
    /// `None` and `insert` is a no-op; counters still tick so an
    /// operator who flips the disable knob mid-flight sees the miss
    /// rate climb to 100 %.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            inner: Mutex::new(PlanCacheInner {
                map: HashMap::new(),
                order: VecDeque::new(),
            }),
            capacity: 0,
            enabled: false,
            generation: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    /// Look the query up in the cache. Returns the cached value
    /// when (a) the cache is enabled, (b) the query has a matching
    /// entry, (c) the entry's generation equals the current
    /// generation. Stale-generation entries are evicted on the spot.
    pub fn lookup(&self, query: &str) -> Option<V> {
        if !self.enabled {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let key = hash_canonicalised(query);
        let current_gen = self.generation.load(Ordering::Acquire);
        let mut inner = self.inner.lock();
        let stale = match inner.map.get(&key) {
            Some(entry) => entry.generation != current_gen,
            None => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        };
        if stale {
            inner.map.remove(&key);
            inner.order.retain(|k| *k != key);
            self.evictions.fetch_add(1, Ordering::Relaxed);
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        // LRU touch: move key to front (most-recent end).
        inner.order.retain(|k| *k != key);
        inner.order.push_front(key);
        let entry = inner.map.get_mut(&key).expect("contains_key check above");
        entry.access_count = entry.access_count.saturating_add(1);
        let value = entry.value.clone();
        drop(inner);
        self.hits.fetch_add(1, Ordering::Relaxed);
        Some(value)
    }

    /// Insert a freshly-computed plan under `query`. Evicts the
    /// least-recently-used entry when the LRU bound is hit. No-op
    /// when the cache is disabled.
    pub fn insert(&self, query: &str, value: V) {
        if !self.enabled {
            return;
        }
        let key = hash_canonicalised(query);
        let generation = self.generation.load(Ordering::Acquire);
        let mut inner = self.inner.lock();
        if inner.map.contains_key(&key) {
            // Update existing — refresh value + generation, move to front.
            inner.order.retain(|k| *k != key);
            inner.order.push_front(key);
            let entry = inner.map.get_mut(&key).expect("present above");
            entry.value = value;
            entry.generation = generation;
            // access_count carries forward — it tracks lifetime hits.
            return;
        }
        if inner.map.len() >= self.capacity {
            if let Some(evicted) = inner.order.pop_back() {
                inner.map.remove(&evicted);
                self.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
        inner.map.insert(
            key,
            CachedEntry {
                value,
                generation,
                access_count: 0,
            },
        );
        inner.order.push_front(key);
    }

    /// Drop every entry. The full-flush API for ops emergencies
    /// (`db.planCache.clear()`); also called internally by
    /// `bump_generation()` if the caller asks for a hard flush.
    /// Counters (hits / misses / evictions) are NOT reset — they
    /// stay monotonic across the process lifetime so an operator
    /// can see hit-rate trend without reset noise.
    pub fn clear(&self) {
        let mut inner = self.inner.lock();
        let n = inner.map.len() as u64;
        inner.map.clear();
        inner.order.clear();
        self.evictions.fetch_add(n, Ordering::Relaxed);
    }

    /// Increment the planner generation. Subsequent lookups for
    /// previously-cached entries will surface as misses (the entry
    /// is evicted on the spot when the lookup notices the
    /// generation mismatch).
    ///
    /// Called from schema-change sites: `CREATE INDEX`,
    /// `DROP INDEX`, `CREATE CONSTRAINT`, `DROP CONSTRAINT`,
    /// label / type / key registry mutations.
    pub fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Snapshot the cache statistics. Cheap — atomic counters +
    /// one mutex acquisition for `size`.
    pub fn stats(&self) -> PlanCacheStats {
        let size = self.inner.lock().map.len();
        PlanCacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            size,
            capacity: self.capacity,
            enabled: self.enabled,
            generation: self.generation.load(Ordering::Acquire),
        }
    }

    /// Return the top `n` entries by `access_count`, descending.
    /// Each tuple is `(canonical_hash, access_count, generation)`
    /// — operator-facing surface for `db.planCache.list()`.
    pub fn top_n(&self, n: usize) -> Vec<(u64, u64, u64)> {
        let inner = self.inner.lock();
        let mut entries: Vec<(u64, u64, u64)> = inner
            .map
            .iter()
            .map(|(k, e)| (*k, e.access_count, e.generation))
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries.truncate(n);
        entries
    }

    /// Current capacity. Exposed so tests + status endpoints can
    /// confirm the env-var wiring without re-reading the env.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// `true` when the cache will admit inserts and serve lookups.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl<V: Clone> Default for PlanCache<V> {
    fn default() -> Self {
        Self::from_env()
    }
}

impl<V> std::fmt::Debug for PlanCache<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanCache")
            .field("capacity", &self.capacity)
            .field("enabled", &self.enabled)
            .field("generation", &self.generation.load(Ordering::Relaxed))
            .field("hits", &self.hits.load(Ordering::Relaxed))
            .field("misses", &self.misses.load(Ordering::Relaxed))
            .field("evictions", &self.evictions.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_canonical() {
        assert!(matches!(canonicalise_query(""), Cow::Borrowed("")));
    }

    #[test]
    fn already_canonical_input_borrows_unchanged() {
        let q = "MATCH (n) RETURN n";
        match canonicalise_query(q) {
            Cow::Borrowed(s) => assert_eq!(s, q),
            Cow::Owned(_) => panic!("expected borrow on canonical input"),
        }
    }

    #[test]
    fn whitespace_runs_collapse_to_single_space() {
        let q = "MATCH (n)   RETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn tabs_and_newlines_collapse_to_single_space() {
        let q = "MATCH (n)\t\n  RETURN\nn";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn leading_and_trailing_whitespace_trimmed() {
        let q = "   MATCH (n) RETURN n   ";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn line_comments_stripped() {
        let q = "MATCH (n) // a node\nRETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn block_comments_stripped_non_nested() {
        let q = "MATCH /* a node */ (n) RETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn block_comment_at_eol_no_terminator_is_consumed_to_eof() {
        // Pathological: unterminated block comment. The
        // canonicaliser silently consumes to EOF — the parser
        // will fail later with a real error message. Document
        // the behaviour so a future test does not surprise.
        let q = "MATCH (n) /* unterminated";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n)");
    }

    #[test]
    fn string_literal_double_quote_preserved_byte_for_byte() {
        let q = "MATCH (n {note: \"a  b   c\"}) RETURN n";
        let c = canonicalise_query(q);
        // Whitespace inside the literal must NOT collapse.
        assert_eq!(c, "MATCH (n {note: \"a  b   c\"}) RETURN n");
    }

    #[test]
    fn string_literal_single_quote_preserved() {
        let q = "MATCH (n {note: 'a  b'}) RETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n {note: 'a  b'}) RETURN n");
    }

    #[test]
    fn string_literal_with_escaped_quote_preserved() {
        let q = r#"MATCH (n) WHERE n.note = "a\"b" RETURN n"#;
        let c = canonicalise_query(q);
        assert_eq!(c, r#"MATCH (n) WHERE n.note = "a\"b" RETURN n"#);
    }

    #[test]
    fn comment_inside_string_literal_not_stripped() {
        let q = "MATCH (n {note: 'a // b'}) RETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n {note: 'a // b'}) RETURN n");
    }

    #[test]
    fn multiple_line_comments_back_to_back() {
        let q = "// header\n// header 2\nMATCH (n) // trailing\nRETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn keyword_case_is_preserved() {
        let q1 = "MATCH (n) RETURN n";
        let q2 = "match (n) return n";
        // Different canonical forms — see module-level docs.
        assert_ne!(canonicalise_query(q1), canonicalise_query(q2));
    }

    #[test]
    fn parameter_names_distinguish_queries() {
        let q1 = "MATCH (n {id: $a}) RETURN n";
        let q2 = "MATCH (n {id: $b}) RETURN n";
        assert_ne!(canonicalise_query(q1), canonicalise_query(q2));
    }

    #[test]
    fn hash_canonicalised_is_stable_across_runs() {
        let h1 = hash_canonicalised("MATCH (n) RETURN n");
        let h2 = hash_canonicalised("MATCH (n) RETURN n");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_canonicalised_collapses_whitespace_variants() {
        let h1 = hash_canonicalised("MATCH (n) RETURN n");
        let h2 = hash_canonicalised("MATCH  (n)\tRETURN\nn");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_canonicalised_collapses_comment_variants() {
        let h1 = hash_canonicalised("MATCH (n) RETURN n");
        let h2 = hash_canonicalised("/* prelude */ MATCH (n) // tail\nRETURN n");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_canonicalised_distinguishes_different_queries() {
        let h1 = hash_canonicalised("MATCH (n) RETURN n");
        let h2 = hash_canonicalised("MATCH (n:Person) RETURN n");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_canonicalised_respects_string_literals() {
        let h1 = hash_canonicalised("MATCH (n {note: 'a b'}) RETURN n");
        let h2 = hash_canonicalised("MATCH (n {note: 'a  b'}) RETURN n");
        // Whitespace inside literal differs → different hashes.
        assert_ne!(h1, h2);
    }

    #[test]
    fn canonical_version_constant_is_pinned() {
        assert_eq!(CANONICAL_VERSION, 1);
    }

    // -----------------------------------------------------------
    // PlanCache tests
    // -----------------------------------------------------------

    #[test]
    fn plan_cache_lookup_returns_inserted_value() {
        let cache: PlanCache<u32> = PlanCache::new(4);
        cache.insert("MATCH (n) RETURN n", 42);
        assert_eq!(cache.lookup("MATCH (n) RETURN n"), Some(42));
        let s = cache.stats();
        assert_eq!(s.hits, 1);
        assert_eq!(s.misses, 0);
        assert_eq!(s.size, 1);
    }

    #[test]
    fn plan_cache_canonicalised_hits_across_whitespace_variants() {
        let cache: PlanCache<u32> = PlanCache::new(4);
        cache.insert("MATCH (n) RETURN n", 7);
        // Whitespace runs collapse to a single space; a comment
        // strips entirely. Both query forms canonicalise to the
        // same key as the inserted form.
        assert_eq!(cache.lookup("MATCH  (n)\tRETURN n"), Some(7));
        assert_eq!(cache.lookup("MATCH (n) /* a node */ RETURN n"), Some(7));
        assert_eq!(cache.stats().hits, 2);
    }

    #[test]
    fn plan_cache_miss_increments_miss_counter() {
        let cache: PlanCache<u32> = PlanCache::new(4);
        assert_eq!(cache.lookup("MATCH (n) RETURN n"), None);
        let s = cache.stats();
        assert_eq!(s.hits, 0);
        assert_eq!(s.misses, 1);
    }

    #[test]
    fn plan_cache_lru_evicts_least_recently_used() {
        let cache: PlanCache<u32> = PlanCache::new(2);
        cache.insert("Q1", 1);
        cache.insert("Q2", 2);
        // Touch Q1 so Q2 becomes LRU.
        let _ = cache.lookup("Q1");
        cache.insert("Q3", 3);
        // Q2 should be the eviction victim.
        assert!(cache.lookup("Q2").is_none());
        assert_eq!(cache.lookup("Q1"), Some(1));
        assert_eq!(cache.lookup("Q3"), Some(3));
        assert!(cache.stats().evictions >= 1);
    }

    #[test]
    fn plan_cache_bump_generation_invalidates_entries() {
        let cache: PlanCache<u32> = PlanCache::new(4);
        cache.insert("MATCH (n) RETURN n", 99);
        cache.bump_generation();
        assert!(cache.lookup("MATCH (n) RETURN n").is_none());
        let s = cache.stats();
        // The miss was a stale-generation eviction, so eviction
        // count bumped alongside the miss.
        assert!(s.evictions >= 1);
        assert!(s.misses >= 1);
        assert_eq!(s.size, 0);
    }

    #[test]
    fn plan_cache_clear_drops_all_entries_but_preserves_counters() {
        let cache: PlanCache<u32> = PlanCache::new(4);
        cache.insert("Q1", 1);
        cache.insert("Q2", 2);
        let _ = cache.lookup("Q1"); // hit
        cache.clear();
        assert!(cache.lookup("Q1").is_none()); // miss after clear
        let s = cache.stats();
        assert_eq!(s.size, 0);
        assert!(s.hits >= 1, "lifetime hit counter must survive clear");
    }

    #[test]
    fn plan_cache_disabled_returns_none_and_no_op_inserts() {
        let cache: PlanCache<u32> = PlanCache::disabled();
        cache.insert("Q1", 1);
        assert!(cache.lookup("Q1").is_none());
        let s = cache.stats();
        assert!(!s.enabled);
        assert_eq!(s.size, 0);
        assert_eq!(s.hits, 0);
        assert_eq!(s.misses, 1);
    }

    #[test]
    fn plan_cache_zero_capacity_is_disabled() {
        let cache: PlanCache<u32> = PlanCache::new(0);
        cache.insert("Q1", 1);
        assert!(cache.lookup("Q1").is_none());
        assert!(!cache.is_enabled());
    }

    #[test]
    fn plan_cache_top_n_orders_by_access_count() {
        let cache: PlanCache<u32> = PlanCache::new(8);
        cache.insert("Q_HOT", 1);
        cache.insert("Q_COLD", 2);
        for _ in 0..5 {
            let _ = cache.lookup("Q_HOT");
        }
        let _ = cache.lookup("Q_COLD"); // 1 hit
        let top = cache.top_n(2);
        assert_eq!(top.len(), 2);
        assert!(top[0].1 >= top[1].1);
        assert_eq!(top[0].1, 5, "top entry should be Q_HOT with 5 hits");
    }

    #[test]
    fn plan_cache_repeated_insert_updates_value_keeps_access_count() {
        let cache: PlanCache<u32> = PlanCache::new(4);
        cache.insert("Q", 1);
        let _ = cache.lookup("Q"); // 1 hit
        cache.insert("Q", 2);
        // The access_count must survive the value update; if it
        // resets, the `top_n` ordering on hot queries becomes
        // jittery whenever the planner repopulates an entry under
        // the same key.
        let top = cache.top_n(1);
        assert_eq!(top[0].1, 1, "access_count must persist across re-insert");
        assert_eq!(cache.lookup("Q"), Some(2));
    }

    #[test]
    fn plan_cache_concurrent_lookups_do_not_race() {
        use std::sync::Arc;
        use std::thread;
        let cache: Arc<PlanCache<u32>> = Arc::new(PlanCache::new(64));
        for i in 0..16 {
            cache.insert(&format!("Q{i}"), i);
        }
        let mut handles = Vec::new();
        for _ in 0..16 {
            let c = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    for i in 0..16 {
                        let v = c.lookup(&format!("Q{i}"));
                        assert_eq!(v, Some(i));
                    }
                }
            }));
        }
        for h in handles {
            h.join().expect("worker panicked");
        }
        let s = cache.stats();
        assert_eq!(s.hits, 16 * 16 * 100);
        assert_eq!(s.misses, 0);
    }

    #[test]
    fn plan_cache_from_env_honours_disable_knob() {
        let prev = std::env::var("NEXUS_PLAN_CACHE_DISABLE").ok();
        // SAFETY: tests within the cache module mutate this env var
        // serially; concurrent tests would conflict but cargo test
        // runs them in the same process and we restore after.
        unsafe { std::env::set_var("NEXUS_PLAN_CACHE_DISABLE", "1") };
        let cache: PlanCache<u32> = PlanCache::from_env();
        assert!(!cache.is_enabled());
        unsafe { std::env::remove_var("NEXUS_PLAN_CACHE_DISABLE") };
        if let Some(v) = prev {
            unsafe { std::env::set_var("NEXUS_PLAN_CACHE_DISABLE", v) };
        }
    }
}
