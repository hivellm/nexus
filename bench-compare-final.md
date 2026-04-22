# Nexus ↔ Neo4j Benchmark Report

Scenarios: **14**

| Classification | Count |
|---|---|
| ⭐ Lead | 14 |
| ✅ Parity | 0 |
| ⚠ Behind | 0 |
| 🚨 Gap | 0 |
| — n/a | 0 |

## aggregation

| Scenario | Nexus p50 (µs) | Nexus p95 (µs) | Neo4j p50 (µs) | Ratio | |
|---|---|---|---|---|---|
| `aggregation.count_all` | 100 | 106 | 1720 | 0.06× | ⭐ |
| `aggregation.min_max_score` | 429 | 906 | 1899 | 0.23× | ⭐ |
| `aggregation.sum_score` | 413 | 434 | 1765 | 0.23× | ⭐ |
| `aggregation.stdev_score` | 167 | 193 | 1837 | 0.09× | ⭐ |

## label_scan

| Scenario | Nexus p50 (µs) | Nexus p95 (µs) | Neo4j p50 (µs) | Ratio | |
|---|---|---|---|---|---|
| `label_scan.count_a` | 172 | 202 | 1608 | 0.11× | ⭐ |
| `label_scan.count_e_with_filter` | 205 | 213 | 1712 | 0.12× | ⭐ |

## scalar

| Scenario | Nexus p50 (µs) | Nexus p95 (µs) | Neo4j p50 (µs) | Ratio | |
|---|---|---|---|---|---|
| `scalar.arithmetic` | 99 | 140 | 1516 | 0.07× | ⭐ |
| `scalar.coalesce` | 107 | 128 | 1537 | 0.07× | ⭐ |
| `scalar.literal_int` | 92 | 116 | 1607 | 0.06× | ⭐ |
| `scalar.string_length` | 102 | 113 | 1658 | 0.06× | ⭐ |
| `scalar.to_upper` | 99 | 117 | 1676 | 0.06× | ⭐ |
| `scalar.string_concat` | 102 | 134 | 1582 | 0.06× | ⭐ |
| `scalar.list_indexing` | 104 | 120 | 1616 | 0.06× | ⭐ |
| `scalar.list_reverse` | 111 | 116 | 1646 | 0.07× | ⭐ |
