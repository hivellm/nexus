# Don't emulate Redis KV commands on the RESP3 port

**Category**: api-design
**Tags**: resp3, api-design, compatibility, anti-pattern

## Description

Nexus speaks RESP3 on port 15476 so `redis-cli` / `iredis` / Grafana work out of the box. That doesn't mean Nexus should pretend to be Redis. Accepting `SET`/`GET`/`HSET`/`EXPIRE` silently would mislead users into thinking Nexus is a KV store with persistence — which it isn't, and the confusing error chain that follows is worse than a crisp rejection at the door. The adopted pattern: **aggressively reject Redis KV commands** with a clear message pointing the user to `HELP` (and to the HTTP endpoint if they really want to store opaque values).

## Example

// nexus-server/src/protocol/resp3/dispatch.rs (pattern):
match cmd.to_uppercase().as_str() {
    "CYPHER" | "PING" | "HEALTH" | "STATS" | "HELLO" | "AUTH"
    | "DB_LIST" | "DB_CREATE" | "DB_DROP" | "DB_USE"
    | "LABELS" | "REL_TYPES" | "PROPERTY_KEYS" | "INDEXES" => { /* dispatch */ }
    "SET" | "GET" | "DEL" | "HSET" | "HGET" | "EXPIRE" | "TTL" => {
        writer.write_error(&format!(
            "ERR unknown command '{cmd}' (Nexus is a graph DB, see HELP)"
        )).await?;
    }
    other => writer.write_error(&format!("ERR unknown command '{other}'")).await?,
}

## When to Use

Not applicable — this is an anti-pattern. The "don't do this" lesson: when you adopt a compatibility layer (RESP3, Bolt, PostgreSQL wire), implement the protocol but **not** the host product's domain semantics. Fail fast on commands that aren't yours so users never build on a lie.

## When NOT to Use

If your product genuinely is a superset of the host (e.g. a Postgres-compatible OLTP DB that actually speaks all of Postgres SQL), ignore this pattern.
