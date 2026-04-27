// Mock data for Nexus GUI — Code/Dependency graph domain
// Uses the nexus-core module structure from DAG.md

const NEXUS_LABELS = [
  { name: 'Module', id: 1, count: 14, color: '#00d4ff' },
  { name: 'Function', id: 2, count: 142, color: '#a78bfa' },
  { name: 'Struct', id: 3, count: 38, color: '#10b981' },
  { name: 'Trait', id: 4, count: 12, color: '#f59e0b' },
  { name: 'Crate', id: 5, count: 3, color: '#ff4d8f' },
];

const NEXUS_RELTYPES = [
  { name: 'DEPENDS_ON', id: 1, count: 47 },
  { name: 'CALLS', id: 2, count: 312 },
  { name: 'IMPLEMENTS', id: 3, count: 28 },
  { name: 'CONTAINS', id: 4, count: 156 },
  { name: 'IMPORTS', id: 5, count: 89 },
];

// Hand-crafted nexus-core module graph, ~80 nodes
const MODULES = [
  'error', 'catalog', 'page_cache', 'storage', 'wal', 'index',
  'transaction', 'executor', 'lib', 'nexus-protocol', 'nexus-server',
];

// Positioned layout (force-directed pre-baked for determinism)
const NODES = [
  // Crate roots (layer 6)
  { id: 1, label: 'Crate', name: 'nexus-server', x: 540, y: 60, props: { version: '0.1.0', edition: '2024' } },
  { id: 2, label: 'Crate', name: 'nexus-core', x: 360, y: 180, props: { version: '0.1.0', edition: '2024' } },
  { id: 3, label: 'Crate', name: 'nexus-protocol', x: 720, y: 180, props: { version: '0.1.0', edition: '2024' } },

  // Modules (layer 0-5)
  { id: 10, label: 'Module', name: 'error', x: 180, y: 620, props: { path: 'nexus-core/src/error.rs', layer: 0 } },
  { id: 11, label: 'Module', name: 'catalog', x: 100, y: 500, props: { path: 'nexus-core/src/catalog/mod.rs', layer: 1 } },
  { id: 12, label: 'Module', name: 'page_cache', x: 240, y: 500, props: { path: 'nexus-core/src/page_cache/mod.rs', layer: 1 } },
  { id: 13, label: 'Module', name: 'storage', x: 380, y: 500, props: { path: 'nexus-core/src/storage/mod.rs', layer: 1 } },
  { id: 14, label: 'Module', name: 'wal', x: 180, y: 400, props: { path: 'nexus-core/src/wal/mod.rs', layer: 2 } },
  { id: 15, label: 'Module', name: 'index', x: 380, y: 400, props: { path: 'nexus-core/src/index/mod.rs', layer: 2 } },
  { id: 16, label: 'Module', name: 'transaction', x: 280, y: 320, props: { path: 'nexus-core/src/transaction/mod.rs', layer: 3 } },
  { id: 17, label: 'Module', name: 'executor', x: 440, y: 280, props: { path: 'nexus-core/src/executor/mod.rs', layer: 4 } },
  { id: 18, label: 'Module', name: 'lib', x: 360, y: 180, props: { path: 'nexus-core/src/lib.rs', layer: 5 }, alias: 'Engine' },

  // Functions inside executor
  { id: 30, label: 'Function', name: 'parse_query', x: 560, y: 340, props: { signature: 'fn(&str) -> Result<Ast>', layer: 4 } },
  { id: 31, label: 'Function', name: 'plan', x: 600, y: 280, props: { signature: 'fn(Ast) -> LogicalPlan', layer: 4 } },
  { id: 32, label: 'Function', name: 'execute', x: 640, y: 340, props: { signature: 'fn(Plan) -> Stream<Row>', layer: 4 } },
  { id: 33, label: 'Function', name: 'optimize', x: 680, y: 260, props: { signature: 'fn(LogicalPlan) -> PhysicalPlan', layer: 4 } },

  // Storage functions
  { id: 40, label: 'Function', name: 'read_node', x: 480, y: 560, props: { signature: 'fn(u64) -> NodeRecord', hot: true } },
  { id: 41, label: 'Function', name: 'write_node', x: 380, y: 620, props: { signature: 'fn(u64, NodeRecord)' } },
  { id: 42, label: 'Function', name: 'read_rel', x: 560, y: 580, props: { signature: 'fn(u64) -> RelRecord', hot: true } },
  { id: 43, label: 'Function', name: 'allocate_page', x: 280, y: 620, props: { signature: 'fn() -> PageId' } },

  // Structs
  { id: 50, label: 'Struct', name: 'NodeRecord', x: 460, y: 460, props: { size: '32 bytes' } },
  { id: 51, label: 'Struct', name: 'RelRecord', x: 540, y: 480, props: { size: '48 bytes' } },
  { id: 52, label: 'Struct', name: 'PropRecord', x: 380, y: 580, props: { size: 'variable' } },
  { id: 53, label: 'Struct', name: 'PageCache', x: 160, y: 560, props: { fields: 4 } },
  { id: 54, label: 'Struct', name: 'NexusServer', x: 620, y: 120, props: { fields: 11 } },
  { id: 55, label: 'Struct', name: 'Engine', x: 440, y: 200, props: { fields: 7 } },
  { id: 56, label: 'Struct', name: 'HnswIndex', x: 460, y: 400, props: { dim: 768 } },
  { id: 57, label: 'Struct', name: 'LabelBitmap', x: 320, y: 440, props: { impl: 'roaring' } },
  { id: 58, label: 'Struct', name: 'WalEntry', x: 180, y: 340, props: { variants: 9 } },

  // Traits
  { id: 70, label: 'Trait', name: 'GraphBuilder', x: 740, y: 300, props: { methods: 4 } },
  { id: 71, label: 'Trait', name: 'PatternDetector', x: 780, y: 380, props: { methods: 3 } },
  { id: 72, label: 'Trait', name: 'Storage', x: 340, y: 540, props: { methods: 6 } },

  // Server handlers
  { id: 80, label: 'Function', name: 'handle_cypher', x: 620, y: 60, props: { route: 'POST /cypher' } },
  { id: 81, label: 'Function', name: 'handle_knn', x: 540, y: 20, props: { route: 'POST /knn_traverse' } },
  { id: 82, label: 'Function', name: 'handle_ingest', x: 700, y: 80, props: { route: 'POST /ingest' } },
  { id: 83, label: 'Function', name: 'handle_schema', x: 760, y: 40, props: { route: 'GET /schema' } },

  // Protocol
  { id: 90, label: 'Function', name: 'mcp_query', x: 820, y: 220, props: { tool: 'nexus/query' } },
  { id: 91, label: 'Function', name: 'umicp_discover', x: 820, y: 140, props: { route: 'GET /umicp/discover' } },

  // Pattern detectors (correlation module)
  { id: 100, label: 'Struct', name: 'PipelineDetector', x: 860, y: 340, props: {} },
  { id: 101, label: 'Struct', name: 'EventDrivenDetector', x: 880, y: 420, props: {} },
  { id: 102, label: 'Struct', name: 'ArchPatternDetector', x: 840, y: 460, props: {} },
];

// Edges
const EDGES = [
  // Crate deps
  { s: 1, d: 2, t: 'DEPENDS_ON' },
  { s: 1, d: 3, t: 'DEPENDS_ON' },

  // Server contains
  { s: 1, d: 54, t: 'CONTAINS' },
  { s: 54, d: 80, t: 'CONTAINS' },
  { s: 54, d: 81, t: 'CONTAINS' },
  { s: 54, d: 82, t: 'CONTAINS' },
  { s: 54, d: 83, t: 'CONTAINS' },

  // Core contains
  { s: 2, d: 18, t: 'CONTAINS' },
  { s: 18, d: 55, t: 'CONTAINS' },

  // Module layer deps
  { s: 11, d: 10, t: 'DEPENDS_ON' },
  { s: 12, d: 10, t: 'DEPENDS_ON' },
  { s: 13, d: 10, t: 'DEPENDS_ON' },
  { s: 13, d: 12, t: 'DEPENDS_ON' },
  { s: 14, d: 13, t: 'DEPENDS_ON' },
  { s: 15, d: 13, t: 'DEPENDS_ON' },
  { s: 15, d: 12, t: 'DEPENDS_ON' },
  { s: 16, d: 13, t: 'DEPENDS_ON' },
  { s: 16, d: 14, t: 'DEPENDS_ON' },
  { s: 17, d: 13, t: 'DEPENDS_ON' },
  { s: 17, d: 15, t: 'DEPENDS_ON' },
  { s: 17, d: 16, t: 'DEPENDS_ON' },
  { s: 17, d: 11, t: 'DEPENDS_ON' },
  { s: 18, d: 17, t: 'DEPENDS_ON' },

  // Executor internals
  { s: 17, d: 30, t: 'CONTAINS' },
  { s: 17, d: 31, t: 'CONTAINS' },
  { s: 17, d: 32, t: 'CONTAINS' },
  { s: 17, d: 33, t: 'CONTAINS' },
  { s: 30, d: 31, t: 'CALLS' },
  { s: 31, d: 33, t: 'CALLS' },
  { s: 33, d: 32, t: 'CALLS' },
  { s: 32, d: 40, t: 'CALLS' },
  { s: 32, d: 42, t: 'CALLS' },

  // Storage
  { s: 13, d: 40, t: 'CONTAINS' },
  { s: 13, d: 41, t: 'CONTAINS' },
  { s: 13, d: 42, t: 'CONTAINS' },
  { s: 13, d: 50, t: 'CONTAINS' },
  { s: 13, d: 51, t: 'CONTAINS' },
  { s: 13, d: 52, t: 'CONTAINS' },
  { s: 13, d: 72, t: 'CONTAINS' },
  { s: 13, d: 41, t: 'CALLS', dup: true },
  { s: 12, d: 53, t: 'CONTAINS' },
  { s: 12, d: 43, t: 'CONTAINS' },
  { s: 40, d: 50, t: 'DEPENDS_ON' },
  { s: 42, d: 51, t: 'DEPENDS_ON' },
  { s: 41, d: 43, t: 'CALLS' },
  { s: 13, d: 72, t: 'IMPLEMENTS' },

  // WAL
  { s: 14, d: 58, t: 'CONTAINS' },

  // Index
  { s: 15, d: 56, t: 'CONTAINS' },
  { s: 15, d: 57, t: 'CONTAINS' },

  // Server handlers call executor
  { s: 80, d: 32, t: 'CALLS' },
  { s: 81, d: 56, t: 'CALLS' },
  { s: 82, d: 41, t: 'CALLS' },
  { s: 83, d: 11, t: 'CALLS' },

  // Protocol
  { s: 3, d: 90, t: 'CONTAINS' },
  { s: 3, d: 91, t: 'CONTAINS' },
  { s: 90, d: 80, t: 'CALLS' },

  // Correlation traits
  { s: 17, d: 70, t: 'CONTAINS' },
  { s: 17, d: 71, t: 'CONTAINS' },
  { s: 70, d: 100, t: 'IMPLEMENTS' },
  { s: 71, d: 100, t: 'IMPLEMENTS' },
  { s: 71, d: 101, t: 'IMPLEMENTS' },
  { s: 71, d: 102, t: 'IMPLEMENTS' },

  // Server imports
  { s: 1, d: 10, t: 'IMPORTS' },
  { s: 2, d: 10, t: 'IMPORTS' },
  { s: 3, d: 10, t: 'IMPORTS' },
];

// Query history
const QUERY_HISTORY = [
  { id: 1, ts: '14:32:08', ms: 12, rows: 14, query: 'MATCH (m:Module) RETURN m.name, m.layer ORDER BY m.layer' },
  { id: 2, ts: '14:28:41', ms: 3, rows: 1, query: 'MATCH (n:Struct {name: "NexusServer"}) RETURN n' },
  { id: 3, ts: '14:25:17', ms: 47, rows: 312, query: 'MATCH ()-[r:CALLS]->() RETURN count(r)' },
  { id: 4, ts: '14:19:02', ms: 21, rows: 28, query: 'MATCH (t:Trait)<-[:IMPLEMENTS]-(s:Struct) RETURN t.name, collect(s.name)' },
  { id: 5, ts: '14:12:55', ms: 8, rows: 9, query: 'CALL vector.knn(\'Function\', $embedding, 10) YIELD node RETURN node.name' },
  { id: 6, ts: '14:08:33', ms: 156, rows: 1024, query: 'MATCH p=(a:Module)-[:DEPENDS_ON*..4]->(b:Module) RETURN p' },
];

// Audit log
const AUDIT_LOG = [
  { ts: '14:32:08.412', level: 'info', user: 'admin', action: 'query.execute', detail: 'cypher • 12ms • 14 rows' },
  { ts: '14:31:55.001', level: 'info', user: 'system', action: 'wal.checkpoint', detail: 'epoch 8421 → 8422 • 2.1 MB flushed' },
  { ts: '14:31:42.330', level: 'warn', user: 'replica-2', action: 'replication.lag', detail: 'lag 847ms exceeds threshold 500ms' },
  { ts: '14:30:18.119', level: 'info', user: 'ingest-bot', action: 'ingest.batch', detail: '1,024 nodes • 2,387 rels' },
  { ts: '14:28:41.204', level: 'info', user: 'admin', action: 'query.execute', detail: 'cypher • 3ms • 1 row' },
  { ts: '14:25:17.882', level: 'info', user: 'admin', action: 'query.execute', detail: 'cypher • 47ms • 312 rows' },
  { ts: '14:24:02.551', level: 'info', user: 'system', action: 'hnsw.rebuild', detail: 'label Function • M=16 • 142 vectors' },
  { ts: '14:22:19.044', level: 'info', user: 'admin', action: 'auth.login', detail: 'api_key=nexus_sk_…a7f2 • ip=127.0.0.1' },
  { ts: '14:20:03.992', level: 'error', user: 'replica-1', action: 'replication.reconnect', detail: 'ECONNRESET → retry 3/5 (backoff 8s)' },
  { ts: '14:18:47.116', level: 'info', user: 'system', action: 'epoch.advance', detail: '8420 → 8421' },
];

// Saved connections
const CONNECTIONS = [
  { name: 'localhost:dev', url: 'http://localhost:15474', status: 'connected', role: 'master', current: true },
  { name: 'staging-primary', url: 'https://nexus.staging.internal:15474', status: 'connected', role: 'master', current: false },
  { name: 'staging-replica-1', url: 'https://nexus-r1.staging.internal:15474', status: 'connected', role: 'replica', current: false },
  { name: 'prod (read-only)', url: 'https://nexus.prod.internal:15474', status: 'idle', role: 'replica', current: false },
  { name: 'local-memgraph-test', url: 'http://localhost:17687', status: 'error', role: 'master', current: false },
];

// Live metric time series (60 samples)
function genSeries(base, variance, trend = 0) {
  const out = [];
  let v = base;
  for (let i = 0; i < 60; i++) {
    v += (Math.random() - 0.5) * variance + trend;
    v = Math.max(0, v);
    out.push(v);
  }
  return out;
}

const METRICS = {
  qps: genSeries(420, 80),
  cacheHit: genSeries(94, 3).map(v => Math.min(100, v)),
  walSize: genSeries(128, 4, 0.1),
  p99Latency: genSeries(18, 6),
};

// Replication topology
const REPLICATION = {
  master: { host: 'nexus-master.prod', epoch: 8422, wal: '128.4 MB', lag: 0, status: 'healthy' },
  replicas: [
    { host: 'nexus-replica-1.prod', epoch: 8422, wal: '128.4 MB', lag: 12, status: 'healthy', ackMs: 4 },
    { host: 'nexus-replica-2.prod', epoch: 8421, wal: '128.1 MB', lag: 847, status: 'degraded', ackMs: 47 },
    { host: 'nexus-replica-3.prod', epoch: 8422, wal: '128.4 MB', lag: 8, status: 'healthy', ackMs: 3 },
  ],
};

// Current query result (for table view) — from "MATCH (m:Module) RETURN m.name, m.layer, m.path"
const RESULT_ROWS = NODES
  .filter(n => n.label === 'Module')
  .map(n => ({
    'm.name': n.name,
    'm.layer': n.props.layer,
    'm.path': n.props.path,
  }))
  .sort((a, b) => a['m.layer'] - b['m.layer']);

Object.assign(window, {
  NEXUS_LABELS, NEXUS_RELTYPES, NODES, EDGES, QUERY_HISTORY,
  AUDIT_LOG, CONNECTIONS, METRICS, REPLICATION, RESULT_ROWS,
});
