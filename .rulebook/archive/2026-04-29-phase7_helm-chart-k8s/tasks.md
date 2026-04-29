## 1. Single-node Helm chart
- [x] 1.1 Scaffold `deploy/helm/nexus/` (Chart.yaml, values.yaml, templates/)
- [x] 1.2 Implement single-node `StatefulSet` template with persistent storage
- [x] 1.3 Implement `Service` exposing HTTP (15474) and RPC (15475)
- [x] 1.4 Implement `ConfigMap` for nexus-server config + `Secret` for keys/certs
- [x] 1.5 Implement liveness probe (`/health`) and readiness probe (`/stats`)
- [x] 1.6 Add `NetworkPolicy` template (default-deny + per-port allow)

## 2. V2 cluster mode
- [x] 2.1 Add multi-shard `StatefulSet` with per-shard PVCs
- [x] 2.2 Add headless `Service` for Raft peer discovery
- [x] 2.3 Wire shard-count + replica-factor as Helm values
- [x] 2.4 Document scaling procedure (add shard, remove shard)

## 3. Docker Compose
- [x] 3.1 Create `deploy/docker-compose/single-node/docker-compose.yml`
- [x] 3.2 Create `deploy/docker-compose/master-replica/docker-compose.yml`
- [x] 3.3 Create `deploy/docker-compose/v2-cluster/docker-compose.yml` (3 shards × 3 replicas)

## 4. Documentation
- [x] 4.1 Create `docs/operations/KUBERNETES.md` — install + upgrade + scaling + backup
- [x] 4.2 Document Prometheus scrape config + sample Grafana dashboard
- [x] 4.3 Document TLS rollout (cert-manager integration)
- [x] 4.4 Cross-link from README

## 5. CI
- [x] 5.1 Add `helm lint` + `helm template` validation step to CI
- [x] 5.2 Add `kind`-based smoke test installing the chart end-to-end

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 6.1 Update or create documentation covering the implementation
- [x] 6.2 Write tests covering the new behavior
- [x] 6.3 Run tests and confirm they pass
