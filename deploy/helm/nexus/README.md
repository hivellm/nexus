# Nexus Helm chart

> Production-ready Kubernetes manifests for the Nexus graph database.

[Nexus](https://github.com/hivellm/nexus) is a high-performance property
graph database with native vector search. This chart provisions
single-node and V2 sharded cluster topologies, with persistent storage,
authentication, optional `NetworkPolicy` and `ServiceMonitor`, and
liveness / readiness probes against the HTTP API.

## TL;DR

```bash
helm install nexus ./deploy/helm/nexus \
  --namespace nexus --create-namespace \
  --set auth.root.password='<strong-password>'
```

Then port-forward and smoke-test:

```bash
kubectl -n nexus port-forward svc/nexus 15474:15474
curl http://localhost:15474/health
```

## Requirements

| Component | Version |
|---|---|
| Kubernetes | ≥ 1.24 |
| Helm | ≥ 3.10 |
| Storage | A `StorageClass` providing `ReadWriteOnce` PVCs (default cluster `StorageClass` works for most environments) |

## Topologies

### Single-node (default)

A single StatefulSet pod with a persistent volume. Suitable for
development, staging, and production deployments where one writer is
sufficient.

```bash
helm install nexus ./deploy/helm/nexus -n nexus --create-namespace
```

### V2 sharded cluster

Sharded topology with Raft consensus per shard. The chart computes
`numShards × replicaFactor` pods, attaches a per-pod PVC, exposes the
headless Service required for Raft peer discovery, and seeds each pod
with the right `NEXUS_SHARDING_*` environment variables.

```bash
helm install nexus ./deploy/helm/nexus \
  -f ./deploy/helm/nexus/values-cluster.yaml \
  -n nexus --create-namespace \
  --set auth.root.password='<strong-password>'
```

After the first install, flip `cluster.bootstrapMode` to `join` for
subsequent `helm upgrade` runs so additional pods join the existing
cluster instead of bootstrapping a new one. See the scaling section in
[`docs/operations/KUBERNETES.md`](../../../docs/operations/KUBERNETES.md).

## Configuration reference

The full schema is in [`values.yaml`](./values.yaml). The most common
overrides:

| Key | Default | Description |
|---|---|---|
| `image.repository` | `hivehub/nexus` | Container image. |
| `image.tag` | `""` (defaults to `appVersion`) | Image tag. |
| `mode` | `standalone` | `standalone` or `cluster`. |
| `replicaCount` | `1` | StatefulSet replicas (standalone only). |
| `cluster.numShards` | `3` | Shard count (cluster only). |
| `cluster.replicaFactor` | `3` | Replicas per shard. |
| `auth.enabled` | `true` | Enforce auth on every listener. |
| `auth.root.password` | `""` | Root password (use `--set` or `existingSecret`). |
| `auth.root.existingSecret` | `""` | Reference an external Secret with `username` + `password` keys. |
| `persistence.size` | `50Gi` | PVC size per pod. |
| `persistence.storageClass` | `""` | Override the default `StorageClass`. |
| `service.type` | `ClusterIP` | `ClusterIP` / `NodePort` / `LoadBalancer`. |
| `networkPolicy.enabled` | `false` | Provision default-deny + allow rules. |
| `prometheus.serviceMonitor.enabled` | `false` | Provision a `ServiceMonitor`. |
| `tls.enabled` | `false` | Mount a TLS Secret at `/app/tls`. |

## Authentication

Authentication is enabled by default. The chart creates a Secret named
`<release>-auth` with two keys:

- `username` — defaults to `admin`.
- `password` — taken from `auth.root.password`, or generated as a
  random 32-character alphanumeric string when the value is empty.

Recover the password:

```bash
kubectl -n nexus get secret nexus-auth -o jsonpath='{.data.password}' | base64 -d
```

Production deployments should reference an externally-managed Secret:

```yaml
auth:
  root:
    existingSecret: my-prod-nexus-auth
```

with the secret containing `username` and `password` keys.

## Probes

| Probe | Path | Purpose |
|---|---|---|
| `startupProbe` | `/health` | Gives the binary up to ~150 s to come up before liveness kicks in. |
| `livenessProbe` | `/health` | Cheap; restarts a wedged pod. |
| `readinessProbe` | `/stats` | Removes a pod from the Service when the catalog is unhealthy. |

`auth.requireHealthAuth` defaults to `false` so the kubelet probes do
not need a token. Flip to `true` only if your environment requires
authenticated probes (and provide the kubelet with a Bearer token via
your runtime).

## Persistence

Each pod gets its own PVC via `volumeClaimTemplates`. The chart never
re-uses PVCs across pods — `ReadWriteOnce` is sufficient. Disabling
persistence (`persistence.enabled: false`) makes the deployment
ephemeral; data is lost on every pod restart.

## TLS

Set `tls.enabled: true` and reference an existing Secret containing
`tls.crt` and `tls.key`. The chart mounts that Secret read-only at
`/app/tls`. Pair with cert-manager — see the cert-manager section in
[`docs/operations/KUBERNETES.md`](../../../docs/operations/KUBERNETES.md).

## Observability

Enable the Prometheus Operator integration:

```yaml
prometheus:
  serviceMonitor:
    enabled: true
```

The Nexus server exposes Prometheus-format metrics at `/metrics`. The
ServiceMonitor scrapes the regular Service every 30 s by default.

## Upgrades

`helm upgrade` rolls pods one at a time. The StatefulSet uses
`RollingUpdate` semantics; persistent data survives upgrades.

```bash
helm upgrade nexus ./deploy/helm/nexus -n nexus
```

For cluster mode, the chart sets `podManagementPolicy: Parallel` so
the cluster bootstraps quickly on first install, but
`updateStrategy: RollingUpdate` ensures upgrades replace one replica
at a time once the cluster is healthy.

## Uninstall

```bash
helm uninstall nexus -n nexus
```

PVCs are NOT deleted automatically — Kubernetes preserves them so you
can recover the data. To delete the data as well:

```bash
kubectl -n nexus delete pvc -l app.kubernetes.io/instance=nexus
```

## Helm tests

```bash
helm test nexus -n nexus
```

Spawns a curl pod that polls `/health` and `/stats` on the Service.

## Source links

- [`docs/operations/KUBERNETES.md`](../../../docs/operations/KUBERNETES.md) — operator runbook.
- [`docs/CLUSTER_MODE.md`](../../../docs/CLUSTER_MODE.md) — multi-tenant cluster mode (distinct from V2 sharding).
- [`Dockerfile`](../../../Dockerfile) — the container image this chart deploys.
