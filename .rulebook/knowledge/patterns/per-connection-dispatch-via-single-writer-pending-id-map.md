# Per-connection dispatch via single writer + pending-id map

**Category**: architecture
**Tags**: rpc, transport, concurrency, sdk, messagepack

## Description

A persistent TCP RPC connection needs writes serialised (frames cannot interleave on the wire) and responses multiplexed back to the right caller (they may arrive out of completion order). The pattern we adopted across all six SDKs is: one writer mutex/lock guarding the socket, a `Map<request_id, channel>` (or `TaskCompletionSource` / `Future` / `Promise`) of pending callers, and a single reader task that reads frames off the socket and dispatches them to the pending map by id. Every SDK uses the same shape — it's simple, correct, and trivially debuggable.

## Example

// Rust (sdks/rust/src/transport/rpc.rs):
pub struct RpcTransport {
    stream: Mutex<Option<BufReader<TcpStream>>>,  // writer lock
    next_id: AtomicU32,                            // skip PUSH_ID (u32::MAX)
}
// TypeScript: Map<number, {resolve, reject}> + single on('data') handler
// Python: asyncio._pending: Dict[int, Future] + a create_task(_read_loop)
// Go: sync.Map of chan RpcResponse + a goroutine for readLoop
// C#: ConcurrentDictionary<uint, TaskCompletionSource> + background Task
// PHP: synchronous write-then-read (process model makes goroutines awkward)

## When to Use

Any persistent-socket request/response protocol where concurrent callers need to share a single connection. Especially right when request ids are already monotonic — you get correctness + O(1) lookup for free.

## When NOT to Use

Connection-per-request protocols (short-lived TCP with one in-flight request per socket, like naive HTTP/1.0). Pub/sub-first protocols where streams don't have a request-id concept. PHP's synchronous model uses a simpler blocking read-after-write pattern — appropriate when concurrency lives in the process model (multiple workers) rather than in-process.
