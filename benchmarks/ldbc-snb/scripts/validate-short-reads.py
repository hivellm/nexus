#!/usr/bin/env python3
"""Differential validation of the LDBC SNB short reads (IS1-IS7) against a
Nexus instance and the Neo4j baseline holding the SAME loaded graph.

The short reads take ids that a real workload discovers from complex-read
results, so there is no substitution-parameter file for them. Instead this
samples real ids from the loaded database (Persons for IS1-IS3, Messages for
IS4-IS7), runs each query with those ids against BOTH engines, and compares the
result sets. Any query Nexus cannot express, errors on, or answers differently
is reported — that is the finding that feeds phase7_opencypher-gap-closure.

Usage:
    python validate-short-reads.py \
        --nexus http://localhost:15474 --neo4j http://localhost:17474
    python validate-short-reads.py --samples 20      # more ids per query

Exit code is non-zero if any query mismatched or errored, so it can gate CI.
"""

import argparse
import json
import sys
import urllib.request
from pathlib import Path

QUERIES_DIR = Path(__file__).resolve().parent.parent / "queries" / "short"

# Which sampled id each query is driven by.
PERSON_DRIVEN = {"is1_person_profile", "is2_recent_messages", "is3_friends"}
MESSAGE_DRIVEN = {
    "is4_message_content",
    "is5_message_creator",
    "is6_message_forum",
    "is7_message_replies",
}
# Queries whose result has no ORDER BY (or only a partial one): compare as an
# unordered multiset of rows rather than positionally.
UNORDERED = {"is1_person_profile", "is4_message_content", "is5_message_creator"}


def http_json(url, body):
    req = urllib.request.Request(
        url, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req) as resp:
        return json.load(resp)


class Nexus:
    def __init__(self, base):
        self.base = base.rstrip("/")

    def run(self, query, params):
        body = http_json(self.base + "/cypher", {"query": query, "parameters": params})
        if body.get("error"):
            raise RuntimeError(f"Nexus error: {body['error']}")
        return body.get("columns", []), body.get("rows", [])


class Neo4j:
    def __init__(self, base, database="neo4j"):
        self.url = f"{base.rstrip('/')}/db/{database}/tx/commit"

    def run(self, query, params):
        body = http_json(
            self.url, {"statements": [{"statement": query, "parameters": params}]}
        )
        errors = body.get("errors") or []
        if errors:
            e = errors[0]
            raise RuntimeError(f"Neo4j error: [{e.get('code')}] {e.get('message')}")
        result = body["results"][0]
        columns = result["columns"]
        rows = [d["row"] for d in result["data"]]
        return columns, rows


def normalize_value(v):
    # Both engines return JSON scalars/arrays. Normalize a float that is really
    # an integer (12.0 == 12) so an id compared as float on one side and int on
    # the other does not spuriously differ.
    if isinstance(v, float) and v.is_integer():
        return int(v)
    if isinstance(v, list):
        return [normalize_value(x) for x in v]
    return v


def normalize_rows(rows, unordered):
    norm = [[normalize_value(c) for c in row] for row in rows]
    if unordered:
        # Sort by a stable JSON key so row order does not matter.
        norm.sort(key=lambda r: json.dumps(r, sort_keys=True, default=str))
    return norm


def sample_ids(neo4j, label, n):
    _, rows = neo4j.run(f"MATCH (n:{label}) RETURN n.id AS id LIMIT $n", {"n": n})
    return [r[0] for r in rows]


def compare(name, query, param_name, param_value, nexus, neo4j):
    params = {param_name: param_value}
    unordered = name in UNORDERED
    try:
        nx_cols, nx_rows = nexus.run(query, params)
    except Exception as e:  # noqa: BLE001 — report any engine error as a finding
        return ("NEXUS_ERROR", f"{param_name}={param_value}: {e}")
    try:
        n4_cols, n4_rows = neo4j.run(query, params)
    except Exception as e:  # noqa: BLE001
        return ("NEO4J_ERROR", f"{param_name}={param_value}: {e}")

    if nx_cols != n4_cols:
        return ("COLUMNS", f"nexus={nx_cols} neo4j={n4_cols}")
    nx = normalize_rows(nx_rows, unordered)
    n4 = normalize_rows(n4_rows, unordered)
    if nx != n4:
        return (
            "ROWS",
            f"{param_name}={param_value}: nexus={len(nx)} rows, neo4j={len(n4)} rows"
            f"\n      nexus[:2]={nx[:2]}\n      neo4j[:2]={n4[:2]}",
        )
    return ("OK", f"{len(nx)} rows")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--nexus", default="http://localhost:15474")
    ap.add_argument("--neo4j", default="http://localhost:17474")
    ap.add_argument("--samples", type=int, default=10)
    args = ap.parse_args()

    nexus = Nexus(args.nexus)
    neo4j = Neo4j(args.neo4j)

    person_ids = sample_ids(neo4j, "Person", args.samples)
    message_ids = sample_ids(neo4j, "Message", args.samples)
    print(f"sampled {len(person_ids)} Person ids, {len(message_ids)} Message ids\n")

    any_fail = False
    for path in sorted(QUERIES_DIR.glob("*.cypher")):
        name = path.stem
        query = path.read_text(encoding="utf-8")
        if name in PERSON_DRIVEN:
            param_name, ids = "personId", person_ids
        elif name in MESSAGE_DRIVEN:
            param_name, ids = "messageId", message_ids
        else:
            print(f"?? {name}: no id source configured")
            any_fail = True
            continue

        statuses = [compare(name, query, param_name, i, nexus, neo4j) for i in ids]
        ok = sum(1 for s, _ in statuses if s == "OK")
        first_bad = next(((s, d) for s, d in statuses if s != "OK"), None)
        if first_bad is None:
            print(f"OK    {name:<24} {ok}/{len(ids)} ids match")
        else:
            any_fail = True
            status, detail = first_bad
            print(f"FAIL  {name:<24} {ok}/{len(ids)} match — first {status}:\n      {detail}")

    print()
    if any_fail:
        print("RESULT: at least one short read diverged or errored — see above.")
        sys.exit(1)
    print("RESULT: all short reads match Neo4j on every sampled id.")


if __name__ == "__main__":
    main()
