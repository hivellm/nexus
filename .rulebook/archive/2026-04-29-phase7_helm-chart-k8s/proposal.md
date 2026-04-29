# Proposal: phase7_helm-chart-k8s

## Why

K8s-native deployment is table-stakes for any 2025+ database product. Neo4j ships an official Helm chart and operator. Memgraph ships a Helm chart + operator. NebulaGraph ships a full operator. ArangoDB ships an operator. Without K8s artifacts, evaluating Nexus on a cloud cluster requires the user to write `Deployment` + `StatefulSet` + `Service` + `PersistentVolumeClaim` + `ConfigMap` + `NetworkPolicy` from scratch — a friction point that loses evaluation deals before they start. The Nexus binary is single-process and 12-factor-compatible; a Helm chart is a few hundred lines.

## What Changes

- Helm chart at `deploy/helm/nexus/` covering single-node (`StatefulSet` + `PVC` + `Service` exposing 15474 + 15475) and V2 cluster (sharded StatefulSet + per-shard PVCs + `Service` + headless service for Raft peering).
- Default values for resource limits, page-cache size, log level.
- ConfigMap + Secret integration for API keys, JWT secret, TLS certs.
- Liveness + readiness probes hitting `/health` and `/stats`.
- Docker Compose example at `deploy/docker-compose/` for local single-node + replication trial.
- Documentation at `docs/operations/KUBERNETES.md` covering install, upgrade, scaling, backup, monitoring (Prometheus scrape config), TLS rollout.
- Optional: minimal K8s operator (CRD + reconcile loop) under `deploy/operator/` for V2 cluster lifecycle. Defer if effort overruns 2 weeks; ship only the chart first.

## Impact

- Affected specs: new `docs/operations/KUBERNETES.md`.
- Affected code: new `deploy/helm/nexus/`, `deploy/docker-compose/`, optional `deploy/operator/`.
- Breaking change: NO.
- User benefit: cloud-native eval unblocked; matches competitor expectations.
