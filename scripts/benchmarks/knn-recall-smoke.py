#!/usr/bin/env python3
"""Generate a tiny synthetic fvecs corpus for end-to-end smoke testing.

The full SIFT1M brute-force ground-truth pass takes minutes — too long
for the post-build sanity check the developer runs after every change.
This helper writes a 256-vector × 16-dimensional synthetic corpus to
disk in the same `fvecs` format the production loaders consume, so
the CLI binary `knn-recall` can be exercised against it in seconds.
"""

from __future__ import annotations

import argparse
import random
import struct
from pathlib import Path


def write_fvecs(path: Path, vectors: list[list[float]]) -> None:
    with path.open("wb") as f:
        for v in vectors:
            f.write(struct.pack("<i", len(v)))
            for x in v:
                f.write(struct.pack("<f", x))


def generate(seed: int, n: int, dim: int) -> list[list[float]]:
    rng = random.Random(seed)
    return [[rng.uniform(-1.0, 1.0) for _ in range(dim)] for _ in range(n)]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path("data/knn-corpora/synthetic"))
    parser.add_argument("--base-count", type=int, default=256)
    parser.add_argument("--query-count", type=int, default=16)
    parser.add_argument("--dim", type=int, default=16)
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    base = generate(args.seed, args.base_count, args.dim)
    queries = generate(args.seed + 1, args.query_count, args.dim)
    write_fvecs(args.out / "base.fvecs", base)
    write_fvecs(args.out / "queries.fvecs", queries)
    print(f"[knn-smoke] wrote base={args.base_count} queries={args.query_count} dim={args.dim} -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
