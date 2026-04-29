#!/usr/bin/env bash
# download_knn_corpora.sh — fetch the public ANN benchmark corpora the
# Nexus KNN recall harness consumes:
#
#   - SIFT1M     (128-d f32)   — INRIA TEXMEX
#   - GloVe-200d (200-d f32)   — Stanford NLP
#
# Usage:
#   bash scripts/benchmarks/download_knn_corpora.sh [--out DIR] [--corpus sift|glove|all]
#
# The script is idempotent: each download is gated on `--no-clobber`.
# Re-running it costs only the HTTP HEADs the mirrors validate.
#
# After the script finishes, run:
#   cargo +nightly run --release -p nexus-knn-bench --bin knn-recall -- sift \
#     --base   $OUT/sift/sift_base.fvecs \
#     --queries $OUT/sift/sift_query.fvecs \
#     --groundtruth $OUT/sift/sift_groundtruth.ivecs

set -euo pipefail

OUT_DIR="data/knn-corpora"
CORPUS="all"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      OUT_DIR="$2"; shift 2 ;;
    --corpus)
      CORPUS="$2"; shift 2 ;;
    -h|--help)
      grep '^#' "$0" | sed 's/^# *//' ; exit 0 ;;
    *)
      echo "[knn-corpora] unknown arg: $1" >&2; exit 2 ;;
  esac
done

mkdir -p "$OUT_DIR"

if ! command -v curl >/dev/null 2>&1; then
  echo "[knn-corpora] curl is required" >&2
  exit 1
fi

fetch() {
  local url="$1" dest="$2"
  if [[ -s "$dest" ]]; then
    echo "[knn-corpora] $dest already present, skipping"
    return 0
  fi
  echo "[knn-corpora] fetching $url -> $dest"
  curl --fail --show-error --location --no-clobber --output "$dest" "$url"
}

extract_tar_gz() {
  local archive="$1" out="$2"
  if [[ -d "$out" && -n "$(ls -A "$out" 2>/dev/null || true)" ]]; then
    echo "[knn-corpora] $out already populated, skipping extraction"
    return 0
  fi
  mkdir -p "$out"
  echo "[knn-corpora] extracting $archive -> $out"
  tar -xzf "$archive" -C "$out" --strip-components=1
}

extract_zip() {
  local archive="$1" out="$2"
  if [[ -d "$out" && -n "$(ls -A "$out" 2>/dev/null || true)" ]]; then
    echo "[knn-corpora] $out already populated, skipping extraction"
    return 0
  fi
  mkdir -p "$out"
  echo "[knn-corpora] extracting $archive -> $out"
  unzip -o -q "$archive" -d "$out"
}

if [[ "$CORPUS" == "sift" || "$CORPUS" == "all" ]]; then
  SIFT_ARCHIVE="$OUT_DIR/sift.tar.gz"
  SIFT_DIR="$OUT_DIR/sift"
  fetch "ftp://ftp.irisa.fr/local/texmex/corpus/sift.tar.gz" "$SIFT_ARCHIVE"
  extract_tar_gz "$SIFT_ARCHIVE" "$SIFT_DIR"
fi

if [[ "$CORPUS" == "glove" || "$CORPUS" == "all" ]]; then
  if ! command -v unzip >/dev/null 2>&1; then
    echo "[knn-corpora] unzip is required for the GloVe corpus" >&2
    exit 1
  fi
  GLOVE_ARCHIVE="$OUT_DIR/glove.6B.zip"
  GLOVE_DIR="$OUT_DIR/glove"
  fetch "https://nlp.stanford.edu/data/glove.6B.zip" "$GLOVE_ARCHIVE"
  extract_zip "$GLOVE_ARCHIVE" "$GLOVE_DIR"
fi

echo "[knn-corpora] OK — corpora available under $OUT_DIR"
