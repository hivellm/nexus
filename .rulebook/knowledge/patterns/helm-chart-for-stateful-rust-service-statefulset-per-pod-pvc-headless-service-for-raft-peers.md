# Helm chart for stateful Rust service: StatefulSet + per-pod PVC + headless Service for Raft peers

**Category**: deployment
**Tags**: helm, kubernetes, statefulset, raft, nexus

## Description

For Rust services with persistent on-disk state and optional Raft consensus (Nexus pattern): use a StatefulSet with `volumeClaimTemplates`, a regular ClusterIP Service for clients, AND a headless Service (clusterIP: None, publishNotReadyAddresses: true) so peers can resolve each other by `<pod>-N.<headless>.<ns>.svc.cluster.local` for Raft peer discovery. Wire the peer list at install time via a templated env var rather than a config file — it's a derivable function of `numShards * replicaFactor` and the release name, so the chart should compute it.

## Example

apiVersion: apps/v1
kind: StatefulSet
spec:
  serviceName: {{ include "nexus.headlessServiceName" . }}
  podManagementPolicy: {{ if eq .Values.mode "cluster" }}Parallel{{ else }}OrderedReady{{ end }}
  volumeClaimTemplates:
    - metadata:
        name: data
      spec:
        accessModes: [ReadWriteOnce]
---
apiVersion: v1
kind: Service
spec:
  clusterIP: None
  publishNotReadyAddresses: true
  ports: [{ name: raft, port: 15480 }]

## When to Use

Any stateful workload that needs stable per-pod DNS, persistent storage, and optional peer-to-peer consensus. Single-binary Rust servers (Nexus, similar) where each pod owns its data and replication is at the application layer.

## When NOT to Use

Stateless workloads (use Deployment). Workloads with shared state on a single PVC (use ReadWriteMany or external storage). Workloads that don't need stable identity across restarts.
