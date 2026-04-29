#!/usr/bin/env bash
# helm-kind-smoke.sh — end-to-end smoke install of the Nexus Helm chart
# against a kind cluster. Used by the `kind-smoke` job in
# .github/workflows/helm-lint.yml.
#
# Prerequisites (provided by the calling workflow):
#   - kind cluster `nexus-helm` already created and reachable via the
#     active kubeconfig.
#   - The image `hivehub/nexus:${IMAGE_TAG}` already side-loaded into
#     the cluster via `kind load docker-image`.
#
# Steps:
#   1. helm install the chart with `replicaCount=1` and `auth` enabled.
#   2. Wait for the StatefulSet rollout.
#   3. Run `helm test` (the chart's test pod hits /health and /stats).
#   4. Capture diagnostics on failure.
#
# Exit codes:
#   0 — chart deployed and `helm test` passed.
#   1 — any step above failed; diagnostics printed to stderr.

set -euo pipefail

NS="${NS:-nexus}"
RELEASE="${RELEASE:-nexus}"
IMAGE_REPOSITORY="${IMAGE_REPOSITORY:-hivehub/nexus}"
IMAGE_TAG="${IMAGE_TAG:-ci}"
ROOT_PASSWORD="${ROOT_PASSWORD:-ci-test-password}"
ROLLOUT_TIMEOUT="${ROLLOUT_TIMEOUT:-300s}"

dump_diagnostics() {
  echo "[helm-kind-smoke] diagnostics — release=$RELEASE namespace=$NS" >&2
  kubectl -n "$NS" get all || true
  kubectl -n "$NS" describe statefulset "$RELEASE" || true
  for pod in $(kubectl -n "$NS" get pods -l "app.kubernetes.io/instance=$RELEASE" -o name 2>/dev/null); do
    echo "[helm-kind-smoke] --- logs $pod ---" >&2
    kubectl -n "$NS" logs --tail=200 "$pod" || true
    kubectl -n "$NS" describe "$pod" || true
  done
  kubectl -n "$NS" get events --sort-by=.lastTimestamp || true
}

trap 'dump_diagnostics' ERR

echo "[helm-kind-smoke] using image=$IMAGE_REPOSITORY:$IMAGE_TAG"

helm install "$RELEASE" deploy/helm/nexus \
  --namespace "$NS" --create-namespace \
  --set "image.repository=$IMAGE_REPOSITORY" \
  --set "image.tag=$IMAGE_TAG" \
  --set "image.pullPolicy=IfNotPresent" \
  --set "auth.root.password=$ROOT_PASSWORD" \
  --set "persistence.size=1Gi" \
  --set "resources.limits.memory=1Gi" \
  --set "resources.requests.memory=256Mi" \
  --wait --timeout "$ROLLOUT_TIMEOUT"

echo "[helm-kind-smoke] waiting for StatefulSet rollout"
kubectl -n "$NS" rollout status "statefulset/$RELEASE" --timeout="$ROLLOUT_TIMEOUT"

echo "[helm-kind-smoke] running helm test"
helm test "$RELEASE" -n "$NS" --timeout "$ROLLOUT_TIMEOUT"

echo "[helm-kind-smoke] OK"
