#!/usr/bin/env bash
# Polls `docker stats` for the nexus-memtest container and appends rows to a CSV.
#
# Usage: ./measure.sh <tag> <duration_secs>
#   tag: label for this run (baseline, phase1, etc.)
#   duration_secs: how long to sample (sampling interval is fixed at 2s)

set -euo pipefail

TAG="${1:?tag required (e.g. baseline, phase1)}"
DURATION="${2:?duration_secs required}"
CONTAINER="nexus-memtest"
OUT_DIR="$(cd "$(dirname "$0")/../../memtest-output" 2>/dev/null && pwd || mkdir -p "$(dirname "$0")/../../memtest-output" && cd "$(dirname "$0")/../../memtest-output" && pwd)"
OUT_FILE="${OUT_DIR}/${TAG}-$(date +%Y%m%d-%H%M%S).csv"

echo "timestamp,tag,mem_usage_bytes,mem_limit_bytes,mem_percent,cpu_percent" > "$OUT_FILE"

START=$(date +%s)
END=$((START + DURATION))
while [ "$(date +%s)" -lt "$END" ]; do
    # docker stats --no-stream output: "name usage / limit cpu%"
    # MemUsage format: "12.34MiB / 512MiB" — convert both to bytes.
    RAW=$(docker stats --no-stream --format '{{.MemUsage}}|{{.MemPerc}}|{{.CPUPerc}}' "$CONTAINER" 2>/dev/null || echo "||")
    USAGE_RAW=$(echo "$RAW" | cut -d'|' -f1 | awk '{print $1}')
    LIMIT_RAW=$(echo "$RAW" | cut -d'|' -f1 | awk '{print $3}')
    MEM_PCT=$(echo "$RAW" | cut -d'|' -f2 | tr -d '%')
    CPU_PCT=$(echo "$RAW" | cut -d'|' -f3 | tr -d '%')

    to_bytes() {
        local v="$1"
        local num="${v%[A-Za-z]*}"
        local unit="${v##*[0-9.]}"
        case "$unit" in
            B|"") echo "$num" ;;
            KiB|kB) awk -v n="$num" 'BEGIN{printf "%d", n*1024}' ;;
            MiB|MB) awk -v n="$num" 'BEGIN{printf "%d", n*1024*1024}' ;;
            GiB|GB) awk -v n="$num" 'BEGIN{printf "%d", n*1024*1024*1024}' ;;
            *) echo "0" ;;
        esac
    }

    USAGE_B=$(to_bytes "$USAGE_RAW")
    LIMIT_B=$(to_bytes "$LIMIT_RAW")

    echo "$(date -u +%Y-%m-%dT%H:%M:%SZ),${TAG},${USAGE_B:-0},${LIMIT_B:-0},${MEM_PCT:-0},${CPU_PCT:-0}" >> "$OUT_FILE"
    sleep 2
done

echo "Wrote $OUT_FILE"
