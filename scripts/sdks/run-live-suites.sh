#!/usr/bin/env bash
# phase10_external-ids-sdks-live-validation — orchestrate the live SDK suites.
#
# Boots a Nexus container with auth disabled, polls /health until ready,
# burns internal id 0 with a sentinel node so SDK rel tests don't trip
# the source_id/target_id == 0 validator, then runs each SDK's live suite
# in series. Tears down at the end.
#
# Usage:
#   bash scripts/sdks/run-live-suites.sh
#
# Env overrides:
#   NEXUS_LIVE_HOST   — default http://localhost:15474; passed through to
#                       every SDK suite that gates on this var.
#   NEXUS_IMAGE       — default nexus-nexus; the Docker image tag to run.
#   SDKS              — default "python typescript go csharp php"; whitelist
#                       of SDKs to run. Useful for iterating one SDK.
#   KEEP_CONTAINER    — set to 1 to leave the container running for debug.

set -euo pipefail

NEXUS_LIVE_HOST="${NEXUS_LIVE_HOST:-http://localhost:15474}"
NEXUS_IMAGE="${NEXUS_IMAGE:-nexus-nexus}"
SDKS="${SDKS:-python typescript go csharp php}"
CONTAINER="nexus-phase10-live"

cleanup() {
    if [ "${KEEP_CONTAINER:-0}" != "1" ]; then
        docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

echo "[phase10] starting container $CONTAINER from image $NEXUS_IMAGE"
docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
docker run -d --name "$CONTAINER" \
    -p 15474:15474 \
    -e NEXUS_AUTH_ENABLED=false \
    -e NEXUS_AUTH_REQUIRED_FOR_PUBLIC=false \
    -e NEXUS_ROOT_ENABLED=false \
    -e RUST_LOG=warn \
    "$NEXUS_IMAGE" >/dev/null

echo "[phase10] waiting for /health"
until curl -s -f "$NEXUS_LIVE_HOST/health" > /dev/null 2>&1; do
    sleep 1
done
echo "[phase10] container healthy"

# Sentinel-node workaround for the pre-phase9 rel validator quirk.
# Internal id 0 ends up on a `_Sentinel` node so any SDK rel test that
# uses the first user-visible node gets id >= 1.
echo "[phase10] burning internal id 0 with a _Sentinel node"
curl -s -X POST "$NEXUS_LIVE_HOST/data/nodes" \
    -H "Content-Type: application/json" \
    -d '{"labels":["_Sentinel"],"properties":{"note":"phase10 rel-validator workaround"}}' \
    >/dev/null

PASS_SDKS=()
FAIL_SDKS=()

run_python() {
    echo
    echo "=== [python] live suite ==="
    pushd sdks/python >/dev/null
    NEXUS_LIVE_HOST="$NEXUS_LIVE_HOST" \
        python -m pytest nexus_sdk/tests/test_external_id_live.py -m live -v
    local rc=$?
    popd >/dev/null
    return $rc
}

run_typescript() {
    echo
    echo "=== [typescript] live suite ==="
    pushd sdks/typescript >/dev/null
    NEXUS_LIVE_HOST="$NEXUS_LIVE_HOST" \
        npx vitest run tests/external-id.live.test.ts
    local rc=$?
    popd >/dev/null
    return $rc
}

run_go() {
    echo
    echo "=== [go] live suite ==="
    pushd sdks/go/test >/dev/null
    NEXUS_LIVE_HOST="$NEXUS_LIVE_HOST" \
        go test -tags=live -vet=off .
    local rc=$?
    popd >/dev/null
    return $rc
}

run_csharp() {
    echo
    echo "=== [csharp] live suite ==="
    pushd sdks/csharp/Tests >/dev/null
    NEXUS_LIVE_HOST="$NEXUS_LIVE_HOST" \
        dotnet test --filter "category=live"
    local rc=$?
    popd >/dev/null
    return $rc
}

run_php() {
    echo
    echo "=== [php] live suite ==="
    pushd sdks/php >/dev/null
    if command -v php >/dev/null 2>&1 && [ -x vendor/bin/phpunit ]; then
        NEXUS_LIVE_HOST="$NEXUS_LIVE_HOST" \
            vendor/bin/phpunit --group live
        local rc=$?
    else
        echo "[php] no local PHP toolchain — running via php:8.3-cli docker image"
        local host_path
        host_path="$(pwd -W 2>/dev/null || pwd)"
        MSYS_NO_PATHCONV=1 docker run --rm --network host \
            -v "$host_path":/app -w /app \
            -e NEXUS_LIVE_HOST="$NEXUS_LIVE_HOST" \
            php:8.3-cli vendor/bin/phpunit --group live
        local rc=$?
    fi
    popd >/dev/null
    return $rc
}

for sdk in $SDKS; do
    fn="run_$sdk"
    if declare -F "$fn" >/dev/null; then
        if "$fn"; then
            PASS_SDKS+=("$sdk")
        else
            FAIL_SDKS+=("$sdk")
        fi
    else
        echo "[phase10] WARNING: unknown sdk '$sdk' (no run_$sdk function)"
        FAIL_SDKS+=("$sdk")
    fi
done

echo
echo "==============================================="
echo "  phase10 live SDK suites"
echo "  PASS: ${PASS_SDKS[*]:-none}"
echo "  FAIL: ${FAIL_SDKS[*]:-none}"
echo "==============================================="

[ ${#FAIL_SDKS[@]} -eq 0 ]
