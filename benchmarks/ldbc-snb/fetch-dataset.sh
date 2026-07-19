#!/usr/bin/env bash
#
# Fetch the pinned LDBC SNB Interactive v1 artifacts for a scale factor.
#
# Downloads are verified against the SHA-256 values pinned in
# dataset-manifest.tsv and cached OUTSIDE the git tree, so re-running is cheap
# and the repository never grows a multi-hundred-megabyte blob.
#
#   ./fetch-dataset.sh                    # SF0.1 (the smoke scale)
#   ./fetch-dataset.sh --scale 1          # SF1 (the reporting scale)
#   ./fetch-dataset.sh --scale all
#   ./fetch-dataset.sh --cache /mnt/bench # override the cache root
#   ./fetch-dataset.sh --verify-only      # re-check cached archives, no network
#   ./fetch-dataset.sh --force            # re-extract even if already unpacked
#
# Cached archives are always re-hashed against the manifest — no flag skips
# checksum verification.
#
# Cache root resolution order: --cache, $LDBC_SNB_CACHE_DIR, ~/.cache/ldbc-snb
#
# Layout produced under <cache>/sf<scale>/:
#   social_network-sf<scale>-CsvCompositeMergeForeign-LongDateFormatter/
#       static/   organisation, place, tag, tagclass
#       dynamic/  person, forum, post, comment + the edge files
#   substitution_parameters-sf<scale>/     query substitution parameters
#   social_network-sf<scale>-numpart-1/    update streams (person + forum)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$SCRIPT_DIR/dataset-manifest.tsv"

SCALE="0.1"
CACHE_ROOT="${LDBC_SNB_CACHE_DIR:-$HOME/.cache/ldbc-snb}"
FORCE=0
VERIFY_ONLY=0
EXTRACT=1

die() {
    echo "error: $*" >&2
    exit 1
}

usage() {
    sed -n '3,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

need_value() {
    [ -n "${2:-}" ] || die "$1 requires a value"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --scale)       need_value "$1" "${2:-}"; SCALE="$2"; shift 2 ;;
        --cache)       need_value "$1" "${2:-}"; CACHE_ROOT="$2"; shift 2 ;;
        --force)       FORCE=1; shift ;;
        --verify-only) VERIFY_ONLY=1; shift ;;
        --no-extract)  EXTRACT=0; shift ;;
        -h|--help)     usage 0 ;;
        *)             echo "unknown argument: $1" >&2; usage 1 ;;
    esac
done

case "$SCALE" in
    0.1|1|all) ;;
    *) die "unsupported scale '$SCALE' (expected 0.1, 1 or all)" ;;
esac

[ -f "$MANIFEST" ] || die "manifest not found: $MANIFEST"
command -v curl >/dev/null 2>&1 || die "curl is required but not on PATH"

# sha256sum on Linux and Git Bash, shasum -a 256 on macOS.
if command -v sha256sum >/dev/null 2>&1; then
    sha256_of() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
    sha256_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
    die "neither sha256sum nor shasum found; cannot verify downloads"
fi

# Extraction needs zstd. Prefer the CLI; fall back to Python's zstandard
# module, which is far more commonly present on developer machines than the
# zstd binary is on Windows.
resolve_extractor() {
    if command -v zstd >/dev/null 2>&1 && tar --help 2>&1 | grep -q -- '--use-compress-program'; then
        EXTRACTOR="tar"
        return
    fi
    for py in python3 python; do
        if command -v "$py" >/dev/null 2>&1 && "$py" -c "import zstandard" >/dev/null 2>&1; then
            EXTRACTOR="python:$py"
            return
        fi
    done
    die "no zstd decompressor available. Install the 'zstd' CLI, or 'pip install zstandard'."
}

extract_archive() {
    archive="$1"
    dest="$2"
    case "$EXTRACTOR" in
        tar)
            tar --use-compress-program=zstd -xf "$archive" -C "$dest"
            ;;
        python:*)
            "${EXTRACTOR#python:}" - "$archive" "$dest" <<'PY'
import sys, tarfile, zstandard

archive, dest = sys.argv[1], sys.argv[2]
with open(archive, "rb") as fh:
    with zstandard.ZstdDecompressor().stream_reader(fh) as reader:
        with tarfile.open(fileobj=reader, mode="r|") as tf:
            # `data` filter rejects absolute paths, `..` traversal and device
            # nodes. Available since Python 3.12; older runtimes fall back to
            # the historical behaviour.
            try:
                tf.extractall(dest, filter="data")
            except TypeError:
                tf.extractall(dest)
PY
            ;;
    esac
}

EXTRACTOR=""
if [ "$EXTRACT" -eq 1 ]; then
    resolve_extractor
fi

ARCHIVE_DIR="$CACHE_ROOT/archives"
mkdir -p "$ARCHIVE_DIR"

echo "LDBC SNB Interactive v1 — scale $SCALE"
echo "cache root: $CACHE_ROOT"
echo

total=0
while IFS=$'\t' read -r m_scale m_kind m_file m_url m_sha m_bytes; do
    case "$m_scale" in ''|\#*) continue ;; esac
    # .gitattributes pins this file to LF, but strip a stray CR anyway: it
    # rides on the last field and would corrupt the byte-size arithmetic.
    m_bytes="${m_bytes%$'\r'}"
    if [ "$SCALE" != "all" ] && [ "$m_scale" != "$SCALE" ]; then
        continue
    fi

    total=$((total + 1))
    archive="$ARCHIVE_DIR/$m_file"
    target_dir="$CACHE_ROOT/sf$m_scale"
    mkdir -p "$target_dir"

    # Cached archives are ALWAYS re-hashed. --force controls re-extraction
    # only; no flag may skip checksum verification.
    if [ -f "$archive" ]; then
        actual="$(sha256_of "$archive")"
        if [ "$actual" = "$m_sha" ]; then
            echo "cached   $m_file ($m_kind)"
        else
            if [ "$VERIFY_ONLY" -eq 1 ]; then
                die "checksum mismatch for $m_file (expected $m_sha, got $actual)"
            fi
            echo "stale    $m_file — checksum mismatch, re-downloading"
            rm -f "$archive"
        fi
    fi

    if [ ! -f "$archive" ]; then
        if [ "$VERIFY_ONLY" -eq 1 ]; then
            die "missing cached archive $m_file (--verify-only)"
        fi
        mb=$((m_bytes / 1048576))
        echo "download $m_file (${mb} MiB)"
        # Download to a temporary name so an interrupted transfer never leaves
        # a truncated file that later looks cached.
        curl -fL --retry 3 --retry-delay 2 --progress-bar -o "$archive.part" "$m_url" ||
            die "download failed: $m_url"
        actual="$(sha256_of "$archive.part")"
        [ "$actual" = "$m_sha" ] ||
            die "checksum mismatch for $m_file: expected $m_sha, got $actual"
        mv "$archive.part" "$archive"
        echo "verified $m_file"
    fi

    if [ "$EXTRACT" -eq 1 ]; then
        stem="${m_file%.tar.zst}"
        if [ -d "$target_dir/$stem" ] && [ "$FORCE" -eq 0 ]; then
            echo "         already extracted -> sf$m_scale/$stem"
        else
            rm -rf "$target_dir/$stem"
            echo "         extracting -> sf$m_scale/$stem"
            extract_archive "$archive" "$target_dir"
        fi
    fi
done < "$MANIFEST"

[ "$total" -gt 0 ] || die "no manifest entries matched scale '$SCALE'"

echo
echo "done — $total artifact(s) ready under $CACHE_ROOT"
echo "export LDBC_SNB_CACHE_DIR=$CACHE_ROOT"
