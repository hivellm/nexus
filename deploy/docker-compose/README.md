# Docker Compose stacks for Nexus

Three reference stacks, smallest to largest:

| Stack | Topology | Use case |
|---|---|---|
| [`single-node/`](./single-node/docker-compose.yml) | 1 server | Local dev, evaluation, integration tests. |
| [`master-replica/`](./master-replica/docker-compose.yml) | 1 master + 1 read replica | Trial replication, read-scaling demos. |
| [`v2-cluster/`](./v2-cluster/docker-compose.yml) | 3 shards × 3 replicas | Trial sharded cluster (Raft per shard, 9 services). |

For Kubernetes, prefer the Helm chart in
[`deploy/helm/nexus/`](../helm/nexus/README.md).

## Bootstrap a stack

Each stack expects a `root_password.txt` next to its
`docker-compose.yml`:

```bash
cd deploy/docker-compose/single-node
echo "$(openssl rand -hex 32)" > root_password.txt
chmod 600 root_password.txt
docker compose up -d
```

The other two stacks bootstrap the same way — substitute the
directory.

## Verify

```bash
curl http://localhost:15474/health    # single-node + v2-cluster node 0
curl http://localhost:25474/health    # master-replica replica, or v2-cluster node 1
curl http://localhost:35474/health    # v2-cluster node 2
```

Authenticated request (replace `<password>` with the contents of
`root_password.txt`):

```bash
curl -X POST http://localhost:15474/cypher \
  -u admin:<password> \
  -H 'Content-Type: application/json' \
  -d '{"query":"RETURN 1 AS one","parameters":{}}'
```

## Tear down

```bash
docker compose down                # keep volumes
docker compose down -v             # delete volumes (data is gone)
```

## Notes

- The replica and cluster stacks bind the secondary nodes to alternate
  host ports (`25474` / `25475`, `35474` / `35475`) so they can run on
  a single host without colliding.
- The cluster stack ships 3 shards × 3 replicas (9 services). Lower
  `NEXUS_SHARDING_REPLICA_FACTOR` to `1` and trim the peer list +
  service list if you only need a 3-node trial cluster. For larger
  topologies prefer the Helm chart, which generates the peer list
  from `numShards * replicaFactor` automatically.
- All stacks ship Docker secrets for the root password; never put a
  password literal in `environment:`.
