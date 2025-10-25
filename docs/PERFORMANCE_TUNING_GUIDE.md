# Nexus Graph Database - Performance Tuning Guide

## Table of Contents

1. [Performance Overview](#performance-overview)
2. [System-level Tuning](#system-level-tuning)
3. [Application Configuration](#application-configuration)
4. [Query Optimization](#query-optimization)
5. [Index Optimization](#index-optimization)
6. [Memory Management](#memory-management)
7. [Storage Optimization](#storage-optimization)
8. [Network Optimization](#network-optimization)
9. [Benchmarking](#benchmarking)
10. [Monitoring and Profiling](#monitoring-and-profiling)

## Performance Overview

Nexus is designed for high-performance graph operations with native vector similarity search. This guide covers optimization strategies for different workloads and deployment scenarios.

### Performance Targets

| Operation | Target QPS | Target Latency |
|-----------|------------|----------------|
| Point Reads | 100,000+ | < 1ms |
| KNN Search | 10,000+ | < 10ms |
| Pattern Traversal | 1,000+ | < 100ms |
| Bulk Ingest | 100,000+ nodes/sec | N/A |
| Complex Queries | 100+ | < 1s |

### Key Performance Factors

1. **Memory**: Sufficient RAM for caching and indexes
2. **Storage**: Fast SSD storage for data persistence
3. **CPU**: Multi-core processors for parallel processing
4. **Network**: Low-latency network for distributed setups
5. **Configuration**: Proper tuning of application parameters

## System-level Tuning

### CPU Optimization

#### CPU Affinity

```bash
# Set CPU affinity for Nexus process
taskset -c 0-7 nexus-server --config /etc/nexus/config.yml

# Or use systemd service with CPU affinity
[Service]
CPUAffinity=0-7
```

#### CPU Governor

```bash
# Set CPU governor to performance mode
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Make it permanent
echo 'GOVERNOR="performance"' | sudo tee -a /etc/default/cpufrequtils
```

#### NUMA Optimization

```bash
# Check NUMA topology
numactl --hardware

# Run Nexus on specific NUMA node
numactl --cpunodebind=0 --membind=0 nexus-server
```

### Memory Optimization

#### Huge Pages

```bash
# Enable huge pages
echo 1024 | sudo tee /proc/sys/vm/nr_hugepages

# Make it permanent
echo 'vm.nr_hugepages=1024' | sudo tee -a /etc/sysctl.conf
```

#### Memory Overcommit

```bash
# Disable memory overcommit for predictable behavior
echo 0 | sudo tee /proc/sys/vm/overcommit_memory

# Make it permanent
echo 'vm.overcommit_memory=0' | sudo tee -a /etc/sysctl.conf
```

#### Swappiness

```bash
# Reduce swappiness to prefer RAM over swap
echo 10 | sudo tee /proc/sys/vm/swappiness

# Make it permanent
echo 'vm.swappiness=10' | sudo tee -a /etc/sysctl.conf
```

### Storage Optimization

#### File System Tuning

```bash
# Mount with optimized options
mount -o noatime,nodiratime,data=writeback /dev/sdb /var/lib/nexus

# Add to /etc/fstab
/dev/sdb /var/lib/nexus ext4 noatime,nodiratime,data=writeback 0 2
```

#### I/O Scheduler

```bash
# Set I/O scheduler to mq-deadline for SSDs
echo mq-deadline | sudo tee /sys/block/sdb/queue/scheduler

# Make it permanent
echo 'GRUB_CMDLINE_LINUX_DEFAULT="elevator=mq-deadline"' | sudo tee -a /etc/default/grub
sudo update-grub
```

#### Disk I/O Limits

```bash
# Set I/O limits for Nexus process
echo "nexus soft rtprio 99" >> /etc/security/limits.conf
echo "nexus hard rtprio 99" >> /etc/security/limits.conf
```

### Network Optimization

#### TCP Tuning

```bash
# Optimize TCP settings
echo 'net.core.rmem_max = 16777216' >> /etc/sysctl.conf
echo 'net.core.wmem_max = 16777216' >> /etc/sysctl.conf
echo 'net.ipv4.tcp_rmem = 4096 87380 16777216' >> /etc/sysctl.conf
echo 'net.ipv4.tcp_wmem = 4096 65536 16777216' >> /etc/sysctl.conf
echo 'net.core.netdev_max_backlog = 5000' >> /etc/sysctl.conf
sysctl -p
```

#### Connection Limits

```bash
# Increase connection limits
echo 'net.core.somaxconn = 65535' >> /etc/sysctl.conf
echo 'net.ipv4.tcp_max_syn_backlog = 65535' >> /etc/sysctl.conf
sysctl -p
```

## Application Configuration

### Server Configuration

```yaml
# config.yml - Performance-optimized settings
server:
  host: "0.0.0.0"
  port: 3000
  workers: 8  # Match CPU cores
  
# Connection pooling
connection_pool:
  max_connections: 1000
  connection_timeout_ms: 5000
  keep_alive_timeout_ms: 30000
```

### Database Configuration

```yaml
database:
  data_dir: "/var/lib/nexus/data"
  catalog_dir: "/var/lib/nexus/catalog"
  wal_dir: "/var/lib/nexus/wal"
  
  # WAL configuration
  wal:
    enabled: true
    sync_mode: "fsync"  # or "none" for better performance
    max_size_mb: 100
    checkpoint_interval_ms: 5000
    
  # Buffer pool configuration
  buffer_pool:
    size_mb: 2048
    page_size_kb: 64
```

### Index Configuration

```yaml
indexes:
  label_index:
    enabled: true
    cache_size_mb: 512
    bloom_filter_bits: 20
    
  knn_index:
    enabled: true
    dimension: 128
    cache_size_mb: 1024
    algorithm: "hnsw"  # Hierarchical Navigable Small World
    m: 16  # HNSW parameter
    ef_construction: 200
    ef_search: 50
```

### Performance Configuration

```yaml
performance:
  # Memory limits
  max_memory_mb: 8192
  
  # Query configuration
  query_timeout_ms: 30000
  max_query_complexity: 1000
  
  # Batch processing
  batch_size: 1000
  batch_timeout_ms: 100
  
  # Caching
  cache:
    enabled: true
    size_mb: 1024
    ttl_seconds: 3600
    
  # Parallel processing
  parallel_workers: 8
  task_queue_size: 10000
```

## Query Optimization

### Cypher Query Best Practices

#### Use LIMIT

```cypher
-- Good: Limited result set
MATCH (n:Person) RETURN n LIMIT 100

-- Bad: Unbounded result set
MATCH (n:Person) RETURN n
```

#### Filter Early

```cypher
-- Good: Filter before traversal
MATCH (n:Person) WHERE n.age > 25
MATCH (n)-[:KNOWS]->(m) 
RETURN n.name, m.name

-- Bad: Filter after traversal
MATCH (n:Person)-[:KNOWS]->(m:Person)
WHERE n.age > 25
RETURN n.name, m.name
```

#### Use Indexes

```cypher
-- Good: Use indexed properties
MATCH (n:Person) WHERE n.id = 123 RETURN n

-- Bad: Use non-indexed properties
MATCH (n:Person) WHERE n.name = "Alice" RETURN n
```

#### Optimize Patterns

```cypher
-- Good: Specific pattern
MATCH (a:Person {id: 1})-[:KNOWS]->(b:Person) 
RETURN b.name

-- Bad: Generic pattern
MATCH (a)-[r]->(b) 
WHERE a.id = 1 AND type(r) = "KNOWS"
RETURN b.name
```

### Vector Query Optimization

#### Dimension Optimization

```cypher
-- Use appropriate vector dimensions
-- 64-128 dimensions for most use cases
-- 256-512 for complex semantic tasks
-- Avoid dimensions > 1024 unless necessary
```

#### Similarity Thresholds

```cypher
-- Use thresholds to reduce computation
MATCH (n:Person) 
WHERE n.vector IS NOT NULL 
  AND n.vector <-> $query_vector < 0.5
RETURN n.name, n.vector <-> $query_vector as similarity
```

#### Batch Vector Operations

```bash
# Use bulk operations for multiple vectors
curl -X POST http://localhost:3000/knn_traverse \
  -H "Content-Type: application/json" \
  -d '{
    "label": "Person",
    "vectors": [
      [0.1, 0.2, 0.3, 0.4],
      [0.2, 0.3, 0.4, 0.5],
      [0.3, 0.4, 0.5, 0.6]
    ],
    "k": 10
  }'
```

## Index Optimization

### Label Index Tuning

```yaml
indexes:
  label_index:
    # Cache configuration
    cache_size_mb: 512
    
    # Bloom filter for fast negative lookups
    bloom_filter_bits: 20
    
    # Compression
    compression: "lz4"
    
    # Preloading
    preload_labels: ["Person", "Company", "Product"]
```

### KNN Index Tuning

```yaml
indexes:
  knn_index:
    # HNSW parameters
    algorithm: "hnsw"
    m: 16              # Number of bi-directional links
    ef_construction: 200  # Size of dynamic candidate list
    ef_search: 50      # Size of dynamic candidate list for search
    
    # Cache configuration
    cache_size_mb: 1024
    
    # Memory mapping
    memory_mapped: true
    
    # Parallel construction
    parallel_construction: true
    construction_threads: 8
```

### Property Index Tuning

```yaml
indexes:
  property_index:
    enabled: true
    
    # B-tree configuration
    btree:
      page_size_kb: 64
      fill_factor: 0.8
      
    # Cache configuration
    cache_size_mb: 256
    
    # Index statistics
    statistics:
      enabled: true
      update_interval_ms: 60000
```

## Memory Management

### Memory Allocation Strategy

```yaml
memory:
  # Heap configuration
  heap:
    initial_size_mb: 2048
    max_size_mb: 8192
    
  # Off-heap configuration
  offheap:
    enabled: true
    size_mb: 4096
    
  # Cache configuration
  cache:
    label_index_cache_mb: 512
    knn_index_cache_mb: 1024
    query_cache_mb: 256
    buffer_pool_mb: 1024
```

### Garbage Collection Tuning

```bash
# JVM-style GC tuning for Rust (if using JNI)
export RUST_LOG=debug
export MALLOC_ARENA_MAX=4
export MALLOC_MMAP_THRESHOLD_=131072
export MALLOC_TRIM_THRESHOLD_=131072
export MALLOC_TOP_PAD_=131072
export MALLOC_MMAP_MAX_=65536
```

### Memory Monitoring

```bash
# Monitor memory usage
watch -n 1 'ps aux | grep nexus-server | head -1'

# Monitor memory by process
cat /proc/$(pgrep nexus-server)/status | grep -E "(VmPeak|VmSize|VmRSS|VmHWM)"

# Monitor memory by NUMA node
numastat -p $(pgrep nexus-server)
```

## Storage Optimization

### Data Layout Optimization

```yaml
storage:
  # File organization
  file_layout:
    nodes_per_file: 1000000
    relationships_per_file: 1000000
    
  # Compression
  compression:
    enabled: true
    algorithm: "lz4"
    level: 3
    
  # Checksums
  checksums:
    enabled: true
    algorithm: "crc32c"
```

### WAL Optimization

```yaml
wal:
  # WAL configuration
  enabled: true
  sync_mode: "fsync"  # "fsync", "none"
  
  # Size limits
  max_size_mb: 100
  max_files: 10
  
  # Checkpointing
  checkpoint_interval_ms: 5000
  checkpoint_threshold_mb: 50
  
  # Compression
  compression: "lz4"
```

### Backup Optimization

```bash
# Incremental backup script
#!/bin/bash
BACKUP_DIR="/backup/nexus"
DATE=$(date +%Y%m%d_%H%M%S)

# Create incremental backup
rsync -av --link-dest="$BACKUP_DIR/latest" \
  /var/lib/nexus/ \
  "$BACKUP_DIR/incremental_$DATE/"

# Update latest symlink
ln -sfn "$BACKUP_DIR/incremental_$DATE" "$BACKUP_DIR/latest"
```

## Network Optimization

### HTTP/2 Configuration

```yaml
server:
  http2:
    enabled: true
    max_concurrent_streams: 1000
    initial_window_size: 65535
    max_frame_size: 16384
```

### Connection Pooling

```yaml
connection_pool:
  # Pool configuration
  max_connections: 1000
  min_connections: 10
  
  # Timeouts
  connection_timeout_ms: 5000
  idle_timeout_ms: 300000
  max_lifetime_ms: 3600000
  
  # Health checks
  health_check_interval_ms: 30000
```

### Load Balancing

```nginx
# Nginx configuration for load balancing
upstream nexus_backend {
    least_conn;
    server nexus1:3000 max_fails=3 fail_timeout=30s;
    server nexus2:3000 max_fails=3 fail_timeout=30s;
    server nexus3:3000 max_fails=3 fail_timeout=30s;
    
    keepalive 32;
}

server {
    listen 80;
    
    location / {
        proxy_pass http://nexus_backend;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        
        # Timeouts
        proxy_connect_timeout 5s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }
}
```

## Benchmarking

### Performance Testing Script

```bash
#!/bin/bash
# performance-test.sh

SERVER_URL="http://localhost:3000"
ITERATIONS=1000

echo "Running Nexus Performance Tests..."

# Test 1: Point reads
echo "Testing point reads..."
time for i in $(seq 1 $ITERATIONS); do
  curl -s -X POST "$SERVER_URL/cypher" \
    -H "Content-Type: application/json" \
    -d '{"query": "MATCH (n:Person) WHERE n.id = 1 RETURN n.name"}' > /dev/null
done

# Test 2: KNN search
echo "Testing KNN search..."
time for i in $(seq 1 100); do
  curl -s -X POST "$SERVER_URL/knn_traverse" \
    -H "Content-Type: application/json" \
    -d '{
      "label": "Person",
      "vector": [0.1, 0.2, 0.3, 0.4],
      "k": 10
    }' > /dev/null
done

# Test 3: Pattern traversal
echo "Testing pattern traversal..."
time for i in $(seq 1 100); do
  curl -s -X POST "$SERVER_URL/cypher" \
    -H "Content-Type: application/json" \
    -d '{"query": "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name LIMIT 10"}' > /dev/null
done

echo "Performance tests completed!"
```

### Load Testing with Apache Bench

```bash
# Install Apache Bench
sudo apt-get install apache2-utils

# Test concurrent requests
ab -n 10000 -c 100 -H "Content-Type: application/json" \
  -p query.json http://localhost:3000/cypher

# Test KNN endpoint
ab -n 1000 -c 50 -H "Content-Type: application/json" \
  -p knn.json http://localhost:3000/knn_traverse
```

### Custom Benchmark Tool

```rust
// benchmark.rs
use std::time::{Duration, Instant};
use tokio::time::timeout;

async fn benchmark_cypher_queries(client: &reqwest::Client, iterations: usize) -> Result<(), Box<dyn std::error::Error>> {
    let query = CypherRequest {
        query: "MATCH (n:Person) WHERE n.id = 1 RETURN n.name".to_string(),
        params: HashMap::new(),
        timeout_ms: Some(1000),
    };
    
    let start = Instant::now();
    let mut success_count = 0;
    
    for _ in 0..iterations {
        match timeout(Duration::from_millis(1000), client.post("http://localhost:3000/cypher").json(&query).send()).await {
            Ok(Ok(response)) if response.status().is_success() => {
                success_count += 1;
            }
            _ => {}
        }
    }
    
    let duration = start.elapsed();
    let qps = success_count as f64 / duration.as_secs_f64();
    
    println!("Cypher queries: {} QPS ({} successful out of {})", qps, success_count, iterations);
    Ok(())
}
```

## Monitoring and Profiling

### Performance Metrics

```yaml
# Prometheus metrics configuration
metrics:
  enabled: true
  port: 9090
  
  # Custom metrics
  custom_metrics:
    - query_execution_time_histogram
    - knn_search_time_histogram
    - memory_usage_gauge
    - cache_hit_rate_gauge
    - index_size_gauge
```

### Profiling Configuration

```yaml
profiling:
  enabled: true
  
  # CPU profiling
  cpu_profiling:
    enabled: true
    sample_rate: 100  # Hz
    
  # Memory profiling
  memory_profiling:
    enabled: true
    sample_interval_ms: 1000
    
  # Query profiling
  query_profiling:
    enabled: true
    slow_query_threshold_ms: 100
    log_slow_queries: true
```

### Monitoring Scripts

```bash
#!/bin/bash
# monitor-performance.sh

while true; do
  # Get system metrics
  CPU_USAGE=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1)
  MEMORY_USAGE=$(free | grep Mem | awk '{printf "%.1f", $3/$2 * 100.0}')
  DISK_USAGE=$(df -h /var/lib/nexus | awk 'NR==2{print $5}' | cut -d'%' -f1)
  
  # Get Nexus metrics
  NEXUS_STATS=$(curl -s http://localhost:3000/stats)
  NODE_COUNT=$(echo $NEXUS_STATS | jq '.label_index.total_nodes')
  REL_COUNT=$(echo $NEXUS_STATS | jq '.catalog.total_types')
  
  # Log metrics
  echo "$(date): CPU: ${CPU_USAGE}%, Memory: ${MEMORY_USAGE}%, Disk: ${DISK_USAGE}%, Nodes: ${NODE_COUNT}, Relationships: ${REL_COUNT}"
  
  sleep 60
done
```

### Performance Dashboard

```yaml
# Grafana dashboard configuration
dashboard:
  panels:
    - title: "Query Performance"
      type: "graph"
      targets:
        - expr: "rate(nexus_query_execution_time_seconds[5m])"
          legend: "Queries per second"
    
    - title: "Memory Usage"
      type: "graph"
      targets:
        - expr: "nexus_memory_usage_bytes"
          legend: "Memory usage"
    
    - title: "Cache Hit Rate"
      type: "singlestat"
      targets:
        - expr: "nexus_cache_hit_rate"
          legend: "Cache hit rate"
```

This performance tuning guide provides comprehensive strategies for optimizing Nexus in various deployment scenarios. Regular monitoring and benchmarking are essential for maintaining optimal performance as your data and usage patterns evolve.




