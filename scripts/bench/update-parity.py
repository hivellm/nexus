#!/usr/bin/env python3
"""In-place rewriter for the Nexus↔Neo4j bench parity section of
`docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`.

Consumes a report.json emitted by `cargo run -p nexus-bench ... --format json`
(schema: see `crates/nexus-bench/src/report/json.rs`) and replaces the
block fenced by the two HTML markers:

    <!-- BEGIN bench-parity ... -->
    ...generated content...
    <!-- END bench-parity -->

Safe to re-run: only the region between the markers is rewritten, so
hand-written copy around the block is preserved.

Usage:
    ./scripts/bench/update-parity.py <report.json> [doc-path]

Exit codes:
    0 — rewrite applied (or already up to date)
    1 — input or doc path missing / unreadable
    2 — markers not found in the doc (schema drift)
    3 — report.json shape invalid
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

BEGIN = "<!-- BEGIN bench-parity"
END = "<!-- END bench-parity -->"

EMOJI = {
    "Lead": "⭐ Lead",
    "Parity": "✅ Parity",
    "Behind": "⚠️ Behind",
    "Gap": "🚨 Gap",
}


def render_section(report: dict[str, Any]) -> str:
    rows = report.get("rows", [])
    schema_version = report.get("schema_version", "?")
    timestamp = report.get("timestamp", "?")
    nexus_version = report.get("nexus_version", "?")
    scenario_count = report.get("scenario_count", len(rows))

    out: list[str] = []
    out.append(f"{BEGIN} (managed by scripts/bench/update-parity.sh — DO NOT EDIT BY HAND) -->")
    out.append(
        f"<!-- generated from report.json @ {timestamp} "
        f"(nexus-bench v{nexus_version}, schema {schema_version}, "
        f"{scenario_count} scenario(s)) -->"
    )
    out.append("")

    if not rows:
        out.append("_No scenarios in the report. Nothing to render._")
        out.append("")
        out.append(END)
        return "\n".join(out)

    out.append(
        "| Scenario | Category | Nexus p50 (µs) | Nexus p95 (µs) | "
        "Neo4j p50 (µs) | Neo4j p95 (µs) | Ratio (n/N) | Classification |"
    )
    out.append(
        "|---|---|---:|---:|---:|---:|---:|---|"
    )

    lead = parity = behind = gap = 0

    for r in rows:
        sid = r.get("scenario_id", "?")
        cat = r.get("category", "?")
        nexus = r.get("nexus", {}) or {}
        nexus_p50 = nexus.get("p50_us", "—")
        nexus_p95 = nexus.get("p95_us", "—")
        neo4j = r.get("neo4j") or {}
        neo4j_p50 = neo4j.get("p50_us", "—") if neo4j else "—"
        neo4j_p95 = neo4j.get("p95_us", "—") if neo4j else "—"
        ratio = r.get("ratio_p50")
        ratio_str = f"{ratio:.2f}" if isinstance(ratio, (int, float)) else "—"
        classification = r.get("classification")
        banner = EMOJI.get(classification, "—") if classification else "—"
        if classification == "Lead":
            lead += 1
        elif classification == "Parity":
            parity += 1
        elif classification == "Behind":
            behind += 1
        elif classification == "Gap":
            gap += 1
        out.append(
            f"| `{sid}` | {cat} | {nexus_p50} | {nexus_p95} | "
            f"{neo4j_p50} | {neo4j_p95} | {ratio_str} | {banner} |"
        )

    out.append("")
    total_classified = lead + parity + behind + gap
    if total_classified > 0:
        out.append(
            f"**Summary** — ⭐ Lead: {lead} · ✅ Parity: {parity} · "
            f"⚠️ Behind: {behind} · 🚨 Gap: {gap} "
            f"(scenarios classified: {total_classified}/{len(rows)})"
        )
    else:
        out.append(
            "_Ratios not computed — the report was run without "
            "`--compare` or the Neo4j side returned no data._"
        )
    out.append("")
    out.append(END)
    return "\n".join(out)


def rewrite(doc: str, new_section: str) -> str:
    begin_idx = doc.find(BEGIN)
    end_idx = doc.find(END)
    if begin_idx < 0 or end_idx < 0 or end_idx < begin_idx:
        raise SystemExit(
            f"markers {BEGIN!r} and {END!r} not found in doc "
            "— was the section deleted by hand?"
        )
    end_idx += len(END)
    return doc[:begin_idx] + new_section + doc[end_idx:]


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 1
    report_path = Path(argv[1])
    doc_path = Path(
        argv[2] if len(argv) >= 3 else "docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md"
    )

    try:
        report_text = report_path.read_text(encoding="utf-8")
    except OSError as e:
        print(f"error: cannot read report {report_path}: {e}", file=sys.stderr)
        return 1
    try:
        doc_text = doc_path.read_text(encoding="utf-8")
    except OSError as e:
        print(f"error: cannot read doc {doc_path}: {e}", file=sys.stderr)
        return 1
    try:
        report = json.loads(report_text)
    except json.JSONDecodeError as e:
        print(f"error: report is not valid JSON: {e}", file=sys.stderr)
        return 3
    if not isinstance(report, dict) or "rows" not in report:
        print("error: report.json missing `rows` field", file=sys.stderr)
        return 3

    new_section = render_section(report)
    try:
        updated = rewrite(doc_text, new_section)
    except SystemExit as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    if updated == doc_text:
        print(f"{doc_path}: already up to date")
        return 0

    doc_path.write_text(updated, encoding="utf-8")
    print(f"{doc_path}: parity section rewritten from {report_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
