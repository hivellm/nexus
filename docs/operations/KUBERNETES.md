# Kubernetes operations

> Operator runbook for deploying, upgrading, scaling, observing, and
> backing up Nexus on Kubernetes.

The reference Helm chart lives at
[`deploy/helm/nexus/`](../../deploy/helm/nexus/README.md). The
chart-level README documents every value; this document covers the
operator workflows that span multiple values and tie the chart into
the surrounding cluster (cert-manager, Prometheus, backup tooling).

For local trial without Kubernetes, see the Compose stacks in
[`deploy/docker-compose/`](../../deploy/docker-compose/README.md).

## 1. Install

### Prerequisites

- Kubernetes ≥ 1.24
- Helm ≥ 3.10
- A `StorageClass` providing `ReadWriteOnce` PVCs (any cloud-default
  CSI driver will do)
- Optional: cert-manager (TLS), Prometheus Operator (metrics),
  external-secrets (Vault / cloud KMS)

### Single-node

```bash
helm install nexus ./deploy/helm/nexus \
  --namespace nexus --create-namespace \
  --set auth.root.password='<strong-password>'
```

The chart auto-generates a 32-character random root password if you
omit `auth.root.password`; recover it with:

```bash
kubectl -n nexus get secret nexus-auth -o jsonpath='{.data.password}' | base64 -d
```

### Sharded cluster

```bash
helm install nexus ./deploy/helm/nexus \
  -f ./deploy/helm/nexus/values-cluster.yaml \
  --namespace nexus --create-namespace \
  --set auth.root.password='<strong-password>'
```

The default cluster preset is 3 shards × 3 replicas (9 pods, each with
its own PVC). Each pod gets a deterministic id (`node-0`, `node-1`,
…); peer discovery uses the headless Service the chart provisions
automatically.

Verify pods are healthy:

```bash
kubectl -n nexus rollout status statefulset/nexus
kubectl -n nexus exec nexus-0 -- curl -fsS http://localhost:15474/health
```

## 2. Upgrade

`helm upgrade` rolls the StatefulSet pod-by-pod (`updateStrategy:
RollingUpdate`). Persistent data survives upgrades.

```bash
helm upgrade nexus ./deploy/helm/nexus \
  --reuse-values \
  --set image.tag=v2.0.0
```

For cluster mode upgrades, flip `cluster.bootstrapMode: join` after
the first install. The first install bootstraps a fresh cluster;
subsequent upgrades must NOT re-bootstrap.

```bash
helm upgrade nexus ./deploy/helm/nexus \
  -f ./deploy/helm/nexus/values-cluster.yaml \
  --set cluster.bootstrapMode=join \
  --set image.tag=v2.0.0
```

Roll back if the new image misbehaves:

```bash
helm rollback nexus 1 -n nexus
```

## 3. Scaling

### Vertical (resource limits)

```bash
helm upgrade nexus ./deploy/helm/nexus --reuse-values \
  --set resources.limits.cpu=4000m \
  --set resources.limits.memory=16Gi
```

The page cache and HNSW indexes benefit linearly from RAM up to the
working-set size; size memory limits to `working_set + 1 GiB
overhead`.

### Horizontal — replica count (standalone)

`replicaCount` only makes sense in standalone mode if you front the
StatefulSet with a load balancer that respects the single-writer
constraint (one pod accepts writes; the others are read replicas
following via WAL shipping). For most users, the cluster preset is the
right path to read-scale.

### Horizontal — shards / replicas (cluster)

Adding a shard or changing the replica factor requires a manual
reshard procedure because data is partitioned by shard id at write
time:

1. Provision a fresh cluster with the target topology
   (`numShards' = numShards + 1` and/or `replicaFactor' = N`).
2. Use the export/import tooling (`docs/operations/REPLICATION.md`) to
   stream data from the old cluster into the new one.
3. Cut over traffic, then decommission the old release.

The chart never resizes a live cluster in place — Kubernetes can scale
`StatefulSet.spec.replicas` up or down, but Nexus's data layout is
stable per shard count, and silently changing the shard count would
strand data on the dropped pods.

### Horizontal — scale the read tier

Add cache-only replicas behind the same Service via
`replicaCount > 1` in standalone mode, or extra replicas-per-shard in
cluster mode. The chart's headless Service exposes per-pod DNS so
read-routing logic in the SDK can target replicas directly.

## 4. Authentication

| Setting | Default | Notes |
|---|---|---|
| `auth.enabled` | `true` | Enforces auth on HTTP, RPC, and RESP3 listeners. |
| `auth.requiredForPublic` | `true` | Rejects anonymous requests when bound to a non-localhost address. Always `true` in K8s. |
| `auth.requireHealthAuth` | `false` | Set to `true` to require a token on `/health`; the kubelet probe will need the token too. |
| `auth.root.disableAfterSetup` | `true` | Disables the chart-provisioned root user after first boot. Provision a replacement admin via the API before the first restart. |

External-secrets reference:

```yaml
auth:
  root:
    existingSecret: nexus-prod-auth   # username + password keys
```

## 5. TLS

Use cert-manager for automatic rotation:

```yaml
# cert-manager Certificate
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: nexus-tls
  namespace: nexus
spec:
  secretName: nexus-tls
  issuerRef:
    name: letsencrypt-prod
    kind: ClusterIssuer
  dnsNames:
    - nexus.example.com
```

Then:

```yaml
# values.yaml
tls:
  enabled: true
  existingSecret: nexus-tls

ingress:
  enabled: true
  className: nginx
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
  hosts:
    - host: nexus.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - hosts:
        - nexus.example.com
      secretName: nexus-tls
```

The chart mounts the Secret read-only at `/app/tls/tls.crt` and
`/app/tls/tls.key`. Configure the server to terminate TLS by setting
`server.configYaml`:

```yaml
server:
  configYaml: |
    [http]
    tls_cert_path = "/app/tls/tls.crt"
    tls_key_path = "/app/tls/tls.key"
```

## 6. Observability

### Prometheus metrics

```yaml
prometheus:
  serviceMonitor:
    enabled: true
    interval: 30s
```

The chart provisions a `ServiceMonitor` selecting the Nexus Service.
The server exposes Prometheus-format metrics at `/metrics`. Key
metrics:

| Metric | Type | Meaning |
|---|---|---|
| `nexus_http_requests_total` | counter | HTTP request count, labelled by route + status. |
| `nexus_http_request_duration_seconds` | histogram | HTTP latency. |
| `nexus_rpc_in_flight` | gauge | RPC requests currently being processed. |
| `nexus_page_cache_hits_total` | counter | Page-cache hits. |
| `nexus_page_cache_misses_total` | counter | Page-cache misses. |
| `nexus_wal_bytes_written_total` | counter | WAL throughput. |
| `nexus_query_executor_active` | gauge | Currently running queries. |

Sample Prometheus rule (alert on cache-miss spike):

```yaml
- alert: NexusPageCacheThrash
  expr: |
    rate(nexus_page_cache_misses_total[5m])
    / rate(nexus_page_cache_hits_total[5m]) > 0.5
  for: 10m
  labels:
    severity: warning
  annotations:
    summary: "Nexus page-cache hit ratio dropped below 50%"
```

### Grafana dashboard

A reference dashboard JSON will ship under
`docs/operations/grafana-dashboards/nexus-overview.json` once the
dashboard is finalised; tracked in the
`phase7_observability-dashboards` proposal.

Until then, a minimal panel set:

- p50 / p95 / p99 of `nexus_http_request_duration_seconds`
- `rate(nexus_http_requests_total)` per route
- Page-cache hit ratio
- WAL bytes written per second
- Pod CPU / memory / disk usage (from `kube_pod_*` metrics)

### Logs

The server logs to stdout at the level set in
`server.logLevel`. Pipe through your cluster's log aggregator
(Loki, Cloud Logging, ElasticSearch). Useful filters:

- `module=nexus_core::executor` — query execution.
- `module=nexus_core::page_cache` — eviction events.
- `module=nexus_server::api` — request-level errors.

## 7. NetworkPolicy

The chart can provision a default-deny `NetworkPolicy` plus per-port
allow rules:

```yaml
networkPolicy:
  enabled: true
  ingressFrom:
    - namespaceSelector:
        matchLabels:
          kubernetes.io/metadata.name: applications
    - podSelector:
        matchLabels:
          app.kubernetes.io/name: ingester
  prometheusNamespace: monitoring
```

The `ingressFrom` field is a list of standard `NetworkPolicyPeer`
objects. Cluster-mode pods automatically allow Raft traffic from
sibling pods of the same release.

## 8. Backup and restore

Nexus does not yet ship a built-in `pg_dump`-style snapshot tool; the
storage layer is memory-mapped, so a consistent backup requires either
quiescing writes or snapshotting the underlying volume.

Recommended pattern (volume snapshot):

1. Provision a `VolumeSnapshotClass` for your CSI driver.
2. Quiesce writes (set `auth.requireHealthAuth=true` and rotate the
   admin token, or scale traffic away at the LB).
3. Take a `VolumeSnapshot` per PVC:

   ```yaml
   apiVersion: snapshot.storage.k8s.io/v1
   kind: VolumeSnapshot
   metadata:
     name: nexus-data-nexus-0-2026-04-29
     namespace: nexus
   spec:
     volumeSnapshotClassName: csi-snapshots
     source:
       persistentVolumeClaimName: data-nexus-0
   ```

4. Resume writes.

Restore: provision a new PVC from the `VolumeSnapshot`, then bind it
to the StatefulSet by pre-creating a PVC named `data-<release>-<ordinal>`
in the target namespace before running `helm install`. The
StatefulSet's `volumeClaimTemplates` adopts a matching pre-existing
PVC instead of provisioning a new one. A first-class
`persistence.existingClaim` value is tracked under the
`phase7_helm-existing-pvc` proposal.

For point-in-time recovery, the WAL replay path
(`docs/specs/wal-mvcc.md`) is authoritative and runs automatically on
boot.

## 9. Day-2 troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| Pods stuck in `Pending` | No `StorageClass` matching `persistence.storageClass`. | `kubectl describe pvc` will surface the binding error; either install a CSI driver or set `persistence.storageClass` to one already present. |
| `CrashLoopBackOff` immediately on boot | Mismatched `NEXUS_SHARDING_*` between bootstrap and join. | Delete the StatefulSet (keeping PVCs) and reinstall with the correct `cluster.bootstrapMode`. |
| `503 Service Unavailable` from probe | Catalog warm-up still running. | Increase `probes.readiness.initialDelaySeconds` or `probes.startup.failureThreshold`. |
| `OOMKilled` during ingest | Page-cache + executor exceeded memory limit. | Raise `resources.limits.memory` or lower `server.configYaml`'s `[storage].page_cache_mb`. |
| Authentication failures after `helm upgrade` | `auth.root.disableAfterSetup` was `true` and the root user is now disabled. | Recover via the replacement admin you should have provisioned, or set `disableAfterSetup: false`, run `helm upgrade`, then provision a new admin and flip back. |

## 10. Reference

- [Helm chart README](../../deploy/helm/nexus/README.md)
- [Docker Compose stacks](../../deploy/docker-compose/README.md)
- [Cluster-mode (multi-tenant)](../CLUSTER_MODE.md) — distinct from V2 sharding
- [Replication](./REPLICATION.md)
