#!/usr/bin/env python3
"""Debug script to check KNOWS relationships"""

import json
import requests

NEXUS_URI = "http://localhost:15474"

def query(cypher):
    try:
        response = requests.post(
            f"{NEXUS_URI}/cypher",
            json={"query": cypher, "parameters": {}},
            headers={"Content-Type": "application/json"},
            timeout=10
        )
        response.raise_for_status()
        return response.json()
    except Exception as e:
        return {"error": str(e)}

print("Checking KNOWS relationships...")
print()

# Check if KNOWS relationship exists
result = query("MATCH ()-[r:KNOWS]->() RETURN count(r) AS cnt")
print("KNOWS count:", result.get("rows", [[0]])[0][0] if result.get("rows") else 0)
print()

# Check all relationships
result = query("MATCH ()-[r]->() RETURN type(r) AS t, count(r) AS cnt ORDER BY t")
print("All relationship types:")
for row in result.get("rows", []):
    print(f"  {row[0]}: {row[1]}")
print()

# Check KNOWS query with full pattern
result = query("MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS src, b.name AS dst")
print(f"KNOWS query returned {len(result.get('rows', []))} rows:")
for i, row in enumerate(result.get("rows", [])):
    print(f"  Row {i}: src={row[0]}, dst={row[1]}")
print()

# Check what nodes have KNOWS relationships
result = query("MATCH (a)-[r:KNOWS]->(b) RETURN a, r, b")
print(f"Full KNOWS query returned {len(result.get('rows', []))} rows:")
for i, row in enumerate(result.get("rows", [])[:5]):
    a = row[0] if len(row) > 0 else None
    r = row[1] if len(row) > 1 else None
    b = row[2] if len(row) > 2 else None
    print(f"  Row {i}: a={a}, r={r}, b={b}")

