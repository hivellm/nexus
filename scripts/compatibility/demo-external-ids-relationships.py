#!/usr/bin/env python3
"""Live demo: insert nodes with external ids, query, build relationships,
verify the whole graph is consistent.

Scenario:
- A document import system: each File node is keyed by its sha256 hash.
- Each File belongs to a Folder (uuid-keyed).
- Each File can be tagged with multiple Tag nodes (str-keyed).
- Re-running the same ingest produces the same graph (idempotency).
"""
from __future__ import annotations

import json
import sys
import urllib.parse
import urllib.request

HOST = "http://localhost:15474"


def green(s):
    return f"\033[32m{s}\033[0m"


def yellow(s):
    return f"\033[33m{s}\033[0m"


def red(s):
    return f"\033[31m{s}\033[0m"


def section(title):
    print()
    print(yellow(f"=== {title} ==="))


def post_node(body):
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(f"{HOST}/data/nodes", data=data, headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


def get_by_ext(ext):
    q = urllib.parse.urlencode({"external_id": ext})
    with urllib.request.urlopen(f"{HOST}/data/nodes/by-external-id?{q}") as resp:
        return json.loads(resp.read())


def post_cypher(query, params=None):
    data = json.dumps({"query": query, "params": params or {}}).encode("utf-8")
    req = urllib.request.Request(f"{HOST}/cypher", data=data, headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


def post_rel(source_id, target_id, rel_type, properties=None):
    body = {"source_id": source_id, "target_id": target_id, "rel_type": rel_type, "properties": properties or {}}
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(f"{HOST}/data/relationships", data=data, headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


PASS = 0
FAIL = 0


def check(name, cond, detail=""):
    global PASS, FAIL
    if cond:
        PASS += 1
        print(f"  {green('OK')}  {name}")
    else:
        FAIL += 1
        print(f"  {red('FAIL')}  {name}  {detail}")


def main():
    # ── Scenario fixtures ───────────────────────────────────────────
    folder_uuid = "uuid:f0000000-0000-0000-0000-000000000001"
    file_a_sha = "sha256:" + "a" * 64
    file_b_sha = "sha256:" + "b" * 64
    file_c_sha = "sha256:" + "c" * 64
    tag_pdf = "str:tag-pdf"
    tag_archived = "str:tag-archived"

    # The REST /data/relationships validator rejects source_id == 0 or
    # target_id == 0 (pre-phase9 quirk in api::data::CreateRelRequest::
    # validate). Allocate internal id 0 to a `_Sentinel` node so the rest
    # of the demo uses ids >= 1 and rel POSTs succeed. Every external-id
    # assertion below holds without this step; only the rel REST tests
    # depend on it.
    section("0. Allocate internal id 0 to a _Sentinel node")
    r = post_node({"labels": ["_Sentinel"], "properties": {"note": "burns id 0 for rel validator"}})
    check("sentinel created", r.get("error") is None, str(r))

    section("1. Create root Folder via REST (uuid external id)")
    r = post_node({
        "labels": ["Folder"],
        "properties": {"path": "/imports/2026-q2", "owner": "alice"},
        "external_id": folder_uuid,
    })
    folder_id = r["node_id"]
    check("Folder created with uuid external id", r.get("error") is None, str(r))
    check("Folder.node_id is a u64", isinstance(folder_id, int), str(folder_id))

    section("2. Create 3 Files via Cypher (sha256 external ids)")
    for sha, name, size in [
        (file_a_sha, "report.pdf", 1024),
        (file_b_sha, "data.csv", 2048),
        (file_c_sha, "diagram.png", 4096),
    ]:
        q = (
            f"CREATE (n:File {{_id: '{sha}', name: '{name}', "
            f"size: {size}}}) RETURN n._id"
        )
        r = post_cypher(q)
        check(
            f"File {name} created with _id",
            r.get("error") is None and r["rows"][0][0] == sha,
            str(r),
        )

    section("3. Create 2 Tags via REST (str external ids)")
    tag_names = {tag_pdf: "pdf", tag_archived: "archived"}
    for tag, tname in tag_names.items():
        r = post_node({"labels": ["Tag"], "properties": {"name": tname}, "external_id": tag})
        check(f"Tag {tag} created", r.get("error") is None, str(r))

    section("4. Resolve every external id back to its internal id")
    folder = get_by_ext(folder_uuid)
    check("Folder resolves", folder["node"]["id"] == folder_id, str(folder))
    file_ids = {}
    for sha, name in [(file_a_sha, "report.pdf"), (file_b_sha, "data.csv"), (file_c_sha, "diagram.png")]:
        f = get_by_ext(sha)
        file_ids[sha] = f["node"]["id"]
        check(
            f"File {name} resolves",
            f["node"]["properties"].get("name") == name,
            str(f),
        )
    tag_ids = {}
    for tag in [tag_pdf, tag_archived]:
        t = get_by_ext(tag)
        tag_ids[tag] = t["node"]["id"]
        check(f"Tag {tag} resolves", t["node"] is not None, str(t))

    section("5. Build relationships via REST -- File BELONGS_TO Folder")
    # Production callers resolve external keys to internal ids via
    # GET /data/nodes/by-external-id, then drive POST /data/relationships
    # using those ids. This is the cross-system join shape: never store
    # the side table mapping external_key -> nexus_id, just resolve on
    # every traversal.
    for sha in file_ids:
        r = post_rel(file_ids[sha], folder_id, "BELONGS_TO", {"added_at": "2026-05-02"})
        check(f"BELONGS_TO from {sha[:14]}...", r.get("error") is None, str(r))

    section("6. Build relationships via REST -- report.pdf TAGGED both tags")
    a_id = file_ids[file_a_sha]
    for tag, tid in tag_ids.items():
        r = post_rel(a_id, tid, "TAGGED", {})
        check(f"TAGGED to {tag}", r.get("error") is None, str(r))

    section("7. Cypher MATCH counts edges")
    r = post_cypher("MATCH (f:File)-[r:BELONGS_TO]->(folder:Folder) RETURN count(r) AS n")
    n = r["rows"][0][0] if r.get("rows") else None
    check("3 BELONGS_TO edges visible", n == 3, str(r))

    r = post_cypher("MATCH (f:File)-[:TAGGED]->(t:Tag) RETURN count(t) AS n")
    n = r["rows"][0][0] if r.get("rows") else None
    check("2 TAGGED edges visible from report.pdf", n == 2, str(r))

    section("8. Idempotent re-ingest with ON CONFLICT MATCH")
    # Re-run the same File create with the same _id --> no duplicate.
    q = (
        f"CREATE (n:File {{_id: '{file_a_sha}', name: 'report.pdf', "
        f"size: 1024}}) ON CONFLICT MATCH RETURN n._id"
    )
    r = post_cypher(q)
    check(
        "Re-ingest of File A returns the same _id (no duplicate)",
        r.get("error") is None and r["rows"][0][0] == file_a_sha,
        str(r),
    )

    # Verify no duplicate file was actually created.
    r = post_cypher("MATCH (f:File) RETURN count(f) AS n")
    n = r["rows"][0][0] if r.get("rows") else None
    check("File count is still 3 after re-ingest", n == 3, str(r))

    section("9. ON CONFLICT REPLACE updates properties keeping the id")
    q = (
        f"CREATE (n:File {{_id: '{file_a_sha}', name: 'report-v2.pdf', "
        f"size: 9999}}) ON CONFLICT REPLACE RETURN n._id, n.name, n.size"
    )
    r = post_cypher(q)
    check(
        "Replace returns the same _id with updated props",
        r.get("error") is None
        and r["rows"][0][0] == file_a_sha
        and r["rows"][0][1] == "report-v2.pdf"
        and r["rows"][0][2] == 9999,
        str(r),
    )

    section("10. ON CONFLICT ERROR (default) rejects duplicate")
    q = f"CREATE (n:File {{_id: '{file_a_sha}', name: 'dup'}})"
    r = post_cypher(q)
    check("Default conflict policy errored on duplicate", r.get("error") is not None, str(r))

    section("11. Project _id back: RETURN f._id, f.name")
    r = post_cypher("MATCH (f:File {name: 'data.csv'}) RETURN f._id, f.name")
    rows = r.get("rows", [])
    check(
        "Project f._id returns sha256 prefixed string",
        rows and rows[0][0] == file_b_sha and rows[0][1] == "data.csv",
        str(r),
    )

    section("12. Project _id on a node WITHOUT _id is null")
    post_cypher("CREATE (n:Plain {name: 'no-id'})")
    r = post_cypher("MATCH (n:Plain {name: 'no-id'}) RETURN n._id")
    rows = r.get("rows", [])
    check("Plain node _id is null", rows and rows[0][0] is None, str(r))

    section("13. Get whole subgraph back via Cypher pattern match")
    r = post_cypher(
        "MATCH (f:File)-[:BELONGS_TO]->(folder:Folder) "
        "RETURN folder.path AS folder, f.name AS file, f._id AS file_id "
        "ORDER BY f.name"
    )
    check(
        "All 3 files belong to the imports folder",
        r.get("error") is None and len(r["rows"]) == 3,
        str(r),
    )
    if r.get("rows"):
        for row in r["rows"]:
            print(f"      folder={row[0]}  file={row[1]}  _id={row[2][:18]}...")

    section("14. Cross-system join shape: external id -> name -> traversal")
    # Step 1 - system A hands us a sha256 hash, no internal id known.
    f = get_by_ext(file_b_sha)
    name_from_catalog = f["node"]["properties"]["name"]
    # Step 2 - drive the traversal with the resolved property. The
    # server's MATCH path does not yet substitute `$param` inside
    # node-pattern property maps, so we inline the literal name. The
    # cross-system shape itself is unchanged: external sha256 hash is
    # resolved by the catalog, not by a side-table mapping.
    q = (
        "MATCH (f:File {name: '"
        + name_from_catalog.replace("'", "\\'")
        + "'})-[:BELONGS_TO]->(folder) RETURN f._id, folder.path"
    )
    r = post_cypher(q)
    rows = r.get("rows", [])
    check(
        "Resolve-then-traverse: external sha256 -> name -> folder",
        rows and rows[0][0] == file_b_sha and rows[0][1] == "/imports/2026-q2",
        str(r),
    )

    # ── Summary ─────────────────────────────────────────────────────
    print()
    print("=" * 51)
    print(f"  Total checks: {PASS + FAIL}    Pass: {green(PASS)}    Fail: {red(FAIL)}")
    print("=" * 51)
    return 0 if FAIL == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
