#!/usr/bin/env python3
"""End-to-end tests for phase9_external-node-ids against a live Nexus container.

Usage: python scripts/compatibility/test-external-ids-docker.py [host:port]
Default: http://localhost:15474
"""
from __future__ import annotations

import json
import sys
import urllib.parse
import urllib.request

HOST = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:15474"

PASS = 0
FAIL = 0
TOTAL = 0


def green(s: str) -> str:
    return f"\033[32m{s}\033[0m"


def red(s: str) -> str:
    return f"\033[31m{s}\033[0m"


def assert_eq(name: str, actual, expected) -> None:
    global PASS, FAIL, TOTAL
    TOTAL += 1
    if actual == expected:
        PASS += 1
        print(f"  {green('PASS')}  {name}")
    else:
        FAIL += 1
        print(f"  {red('FAIL')}  {name}")
        print(f"        expected: {expected!r}")
        print(f"        actual:   {actual!r}")


def assert_truthy(name: str, value) -> None:
    global PASS, FAIL, TOTAL
    TOTAL += 1
    if value:
        PASS += 1
        print(f"  {green('PASS')}  {name}")
    else:
        FAIL += 1
        print(f"  {red('FAIL')}  {name}: got {value!r}")


def post_node(body: dict) -> dict:
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        f"{HOST}/data/nodes",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


def get_by_ext(ext: str) -> dict:
    q = urllib.parse.urlencode({"external_id": ext})
    with urllib.request.urlopen(f"{HOST}/data/nodes/by-external-id?{q}") as resp:
        return json.loads(resp.read())


def post_cypher(query: str, parameters: dict | None = None) -> dict:
    # Server's CypherRequest uses `params` (not `parameters`) per
    # crates/nexus-server/src/api/cypher/mod.rs:CypherRequest.
    data = json.dumps({"query": query, "params": parameters or {}}).encode("utf-8")
    req = urllib.request.Request(
        f"{HOST}/cypher",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


def health() -> dict:
    with urllib.request.urlopen(f"{HOST}/health") as resp:
        return json.loads(resp.read())


def main() -> int:
    global PASS, FAIL

    print()
    print("=== phase9 external-node-ids — Docker e2e suite ===")
    print(f"    target: {HOST}")
    print()

    # 1. Health
    print("[1] Health check")
    h = health()
    assert_truthy("health endpoint reachable", h)

    # 2. Sha256 create
    print("\n[2] REST POST /data/nodes — sha256 external id")
    sha = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
    r = post_node({"labels": ["FileSha"], "properties": {"name": "a.txt"}, "external_id": sha})
    assert_eq("create with sha256 external id", r.get("error"), None)
    node_id = r.get("node_id")

    # 3. Round-trip GET
    print("\n[3] REST GET /data/nodes/by-external-id")
    g = get_by_ext(sha)
    assert_eq("round-trip resolves same internal id", g.get("node", {}).get("id"), node_id)

    # 4. UUID variant
    print("\n[4] UUID variant")
    uuid = "uuid:11111111-1111-1111-1111-111111111111"
    r = post_node({"labels": ["FileUuid"], "properties": {}, "external_id": uuid})
    assert_eq("uuid variant accepted", r.get("error"), None)

    # 5. Str variant
    print("\n[5] Str variant")
    s = "str:my-natural-key-doc-42"
    r = post_node({"labels": ["FileStr"], "properties": {}, "external_id": s})
    assert_eq("str variant accepted", r.get("error"), None)

    # 6. Bytes variant
    print("\n[6] Bytes variant")
    b = "bytes:deadbeef"
    r = post_node({"labels": ["FileBytes"], "properties": {}, "external_id": b})
    assert_eq("bytes variant accepted", r.get("error"), None)

    # 7. Blake3 variant
    print("\n[7] Blake3 variant")
    b3 = "blake3:2222222222222222222222222222222222222222222222222222222222222222"
    r = post_node({"labels": ["FileB3"], "properties": {}, "external_id": b3})
    assert_eq("blake3 variant accepted", r.get("error"), None)

    # 8. Sha512 variant
    print("\n[8] Sha512 variant")
    s512 = "sha512:" + ("3" * 128)
    r = post_node({"labels": ["FileS512"], "properties": {}, "external_id": s512})
    assert_eq("sha512 variant accepted", r.get("error"), None)

    # 9. Conflict policy ERROR (default)
    print("\n[9] Conflict policy ERROR (default) on duplicate")
    r = post_node({"labels": ["FileSha"], "properties": {"name": "dup"}, "external_id": sha})
    assert_truthy("duplicate rejected", r.get("error"))

    # 10. Conflict policy MATCH
    print("\n[10] Conflict policy MATCH returns existing id")
    r = post_node({"labels": ["FileSha"], "properties": {"name": "ignored"}, "external_id": sha, "conflict_policy": "match"})
    assert_eq("match returns existing id", r.get("node_id"), node_id)

    # 11. Conflict policy REPLACE
    print("\n[11] Conflict policy REPLACE keeps id")
    r = post_node({"labels": ["FileSha"], "properties": {"name": "updated"}, "external_id": sha, "conflict_policy": "replace"})
    assert_eq("replace returns same id", r.get("node_id"), node_id)

    # 12. Cypher CREATE with _id literal + RETURN n._id
    print("\n[12] Cypher CREATE with _id string literal")
    cyp_id = "sha256:4444444444444444444444444444444444444444444444444444444444444444"
    r = post_cypher(f"CREATE (n:Doc {{_id: '{cyp_id}', name: 'cypher_lit'}}) RETURN n._id")
    rows = r.get("rows", [])
    val = rows[0][0] if rows and rows[0] else None
    assert_eq("RETURN n._id projects prefixed string", val, cyp_id)

    # 13. Cypher CREATE with $_id parameter
    print("\n[13] Cypher CREATE with parameter _id")
    pid = "uuid:55555555-5555-5555-5555-555555555555"
    r = post_cypher("CREATE (n:Doc {_id: $ext_id, name: 'cypher_param'}) RETURN n._id", {"ext_id": pid})
    rows = r.get("rows", [])
    val = rows[0][0] if rows and rows[0] else None
    assert_eq("param-form projects same prefixed string", val, pid)

    # 14. Cypher ON CONFLICT MATCH idempotent
    print("\n[14] Cypher CREATE ... ON CONFLICT MATCH idempotent")
    on_match = "uuid:66666666-6666-6666-6666-666666666666"
    r1 = post_cypher(f"CREATE (n:Doc {{_id: '{on_match}'}}) ON CONFLICT MATCH RETURN n._id")
    r2 = post_cypher(f"CREATE (n:Doc {{_id: '{on_match}'}}) ON CONFLICT MATCH RETURN n._id")
    v1 = r1.get("rows", [[None]])[0][0]
    v2 = r2.get("rows", [[None]])[0][0]
    assert_eq("first run returns external id", v1, on_match)
    assert_eq("second run also returns external id (no error)", v2, on_match)

    # 15. Cypher ON CONFLICT REPLACE
    print("\n[15] Cypher CREATE ... ON CONFLICT REPLACE")
    r = post_cypher(f"CREATE (n:Doc {{_id: '{on_match}', tag: 'replaced'}}) ON CONFLICT REPLACE RETURN n._id")
    v = r.get("rows", [[None]])[0][0]
    assert_eq("replace returns same external id", v, on_match)

    # 16. Cypher ON CONFLICT ERROR explicit
    print("\n[16] Cypher CREATE ... ON CONFLICT ERROR rejects duplicate")
    r = post_cypher(f"CREATE (n:Doc {{_id: '{on_match}'}}) ON CONFLICT ERROR")
    assert_truthy("ON CONFLICT ERROR rejected duplicate", r.get("error"))

    # 17. RETURN n._id null when unset
    print("\n[17] RETURN n._id is null when no external id was set")
    post_cypher("CREATE (n:NoExt {name: 'plain'})")
    r = post_cypher("MATCH (n:NoExt {name: 'plain'}) RETURN n._id")
    rows = r.get("rows", [])
    val = rows[0][0] if rows and rows[0] else "MISSING"
    assert_eq("n._id is null on plain node", val, None)

    # 18. Invalid external_id format
    print("\n[18] Invalid external_id format rejected")
    r = post_node({"labels": ["X"], "properties": {}, "external_id": "not-a-real-prefix:zzz"})
    err = r.get("error", "")
    assert_truthy("invalid external_id rejected", "Invalid external_id" in (err or ""))

    # 19. Invalid conflict_policy
    print("\n[19] Invalid conflict_policy rejected")
    r = post_node({"labels": ["X"], "properties": {}, "external_id": "uuid:77777777-7777-7777-7777-777777777777", "conflict_policy": "ignore"})
    err = r.get("error", "")
    assert_truthy("invalid conflict_policy rejected", "Invalid conflict_policy" in (err or ""))

    # 20. GET by absent external id
    print("\n[20] GET by absent external id")
    absent = "uuid:88888888-8888-8888-8888-888888888888"
    g = get_by_ext(absent)
    assert_eq("absent external id returns null node", g.get("node"), None)

    # 21. Cypher with invalid _id format errors at execute time
    print("\n[21] Cypher invalid _id format errors at execute time")
    r = post_cypher("CREATE (n:X {_id: 'not-a-real-prefix:zz'})")
    assert_truthy("invalid _id surface a runtime error", r.get("error"))

    # 22. Bytes-too-long validation
    print("\n[22] Bytes <= 64 byte cap enforced")
    too_long_bytes = "bytes:" + ("ff" * 65)  # 65 bytes
    r = post_node({"labels": ["X"], "properties": {}, "external_id": too_long_bytes})
    assert_truthy("oversize bytes rejected", r.get("error"))

    # 23. Str-too-long validation
    print("\n[23] Str <= 256 byte cap enforced")
    too_long_str = "str:" + ("a" * 257)
    r = post_node({"labels": ["X"], "properties": {}, "external_id": too_long_str})
    assert_truthy("oversize str rejected", r.get("error"))

    # 24. Empty external_id rejected
    print("\n[24] Empty external_id rejected")
    r = post_node({"labels": ["X"], "properties": {}, "external_id": "uuid:"})
    assert_truthy("empty uuid rejected", r.get("error"))

    # Summary
    print()
    print("=" * 51)
    print(f"  Total: {TOTAL}    Pass: {PASS}    Fail: {FAIL}")
    print("=" * 51)
    return 0 if FAIL == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
