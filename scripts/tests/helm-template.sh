#!/usr/bin/env bash
# helm-template.sh — render the Nexus Helm chart with several preset
# value combinations and assert the rendered manifests contain the
# resources we expect. Used by .github/workflows/helm-lint.yml to gate
# every PR that touches deploy/helm/.
#
# Exit codes:
#   0 — every render produced the expected resources.
#   1 — `helm template` failed, or a required resource is missing.
#
# The script is idempotent and writes its rendered manifests to
# `.helm-rendered.yaml` at repo root for downstream tools (kubeconform).

set -euo pipefail

CHART="${CHART:-deploy/helm/nexus}"
OUT_FILE="${OUT_FILE:-.helm-rendered.yaml}"

if ! command -v helm >/dev/null 2>&1; then
  echo "[helm-template] helm not found on PATH" >&2
  exit 1
fi

assert_kind() {
  local file="$1" kind="$2"
  if ! grep -qE "^kind: ${kind}$" "$file"; then
    echo "[helm-template] FAIL: expected kind=$kind in $file" >&2
    return 1
  fi
}

render() {
  local label="$1"; shift
  local out="$1"; shift
  echo "[helm-template] rendering: $label"
  helm template nexus "$CHART" "$@" >"$out"
  echo "[helm-template]   -> $(wc -l <"$out") lines, $(grep -cE '^kind: ' "$out" || true) resources"
}

# 1. Default values (standalone, auth on, no NetworkPolicy).
render "defaults" "$OUT_FILE" \
  --namespace nexus

assert_kind "$OUT_FILE" StatefulSet
assert_kind "$OUT_FILE" Service
assert_kind "$OUT_FILE" Secret
assert_kind "$OUT_FILE" ServiceAccount

# 2. Cluster preset.
TMP_CLUSTER="$(mktemp)"
trap 'rm -f "$TMP_CLUSTER"' EXIT

render "cluster preset" "$TMP_CLUSTER" \
  --namespace nexus \
  -f "$CHART/values-cluster.yaml" \
  --set auth.root.password=ci-test-password

assert_kind "$TMP_CLUSTER" StatefulSet
assert_kind "$TMP_CLUSTER" Service
assert_kind "$TMP_CLUSTER" NetworkPolicy
assert_kind "$TMP_CLUSTER" PodDisruptionBudget

# 3. NetworkPolicy + ServiceMonitor + Ingress + TLS toggled on.
TMP_FULL="$(mktemp)"
render "full feature set" "$TMP_FULL" \
  --namespace nexus \
  --set networkPolicy.enabled=true \
  --set prometheus.serviceMonitor.enabled=true \
  --set ingress.enabled=true \
  --set tls.enabled=true \
  --set tls.existingSecret=nexus-tls \
  --set service.headless.enabled=true \
  --set podDisruptionBudget.enabled=true

assert_kind "$TMP_FULL" StatefulSet
assert_kind "$TMP_FULL" NetworkPolicy
assert_kind "$TMP_FULL" ServiceMonitor
assert_kind "$TMP_FULL" Ingress
assert_kind "$TMP_FULL" PodDisruptionBudget

echo "[helm-template] OK"
