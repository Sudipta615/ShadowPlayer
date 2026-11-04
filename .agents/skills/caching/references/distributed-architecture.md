# Phase 11: Distributed Cache Architecture

23. **Design the distributed cache topology.** For production systems, a single Redis instance is a single point of failure and a scalability limitation:

    **Redis Sentinel (high availability without sharding)**:
    - A master-replica setup with Sentinel processes monitoring the master. If the master fails, Sentinel promotes a replica to master automatically.
    - Architecture: 1 master + 1-2 replicas + 3 Sentinel instances (odd number for quorum).
    - **Use when**: The data fits on a single instance (< available memory of the largest practical instance). You need HA but not horizontal scalability.
    - **Failover behavior**: Sentinel detects master failure (configurable timeout, e.g., 5 seconds) → elects a new master → application clients reconnect (clients must use Sentinel-aware connection libraries). Failover typically takes 5-30 seconds. During failover, writes fail; reads may succeed from replicas if the client supports read-from-replica.
    - **Data loss risk**: Asynchronous replication means writes acknowledged by the master may not have been replicated to the replica at failover time. Data written in the last few milliseconds before failure may be lost. For caches, this is acceptable (the data can be reloaded from the source). For sessions or locks, consider `WAIT` command for synchronous replication of critical writes.

    **Redis Cluster (horizontal scaling + HA)**:
    - Data is automatically partitioned across multiple master nodes using hash slots (16,384 slots distributed across masters). Each master has one or more replicas.
    - Architecture: Minimum 3 masters + 3 replicas (6 instances). Each master owns a subset of the 16,384 hash slots.
    - **Use when**: Data volume exceeds single-instance memory. Write throughput exceeds single-instance capacity. You need horizontal scalability.
    - **Key distribution**: Keys are assigned to hash slots based on `CRC16(key) % 16384`. Related keys can be co-located on the same shard using hash tags: `{user:123}:profile` and `{user:123}:settings` both hash to the slot for `user:123`. This is important for multi-key operations (MGET, transactions, Lua scripts), which only work on keys in the same hash slot.
    - **Limitations**: Multi-key operations across different hash slots fail or require application-level handling. Lua scripts can only access keys on the same shard. Pub/sub works but messages are broadcast to all nodes (not filtered by shard). Database selection (`SELECT`) is not supported (only database 0).
    - **Scaling**: Add new masters and rebalance hash slots (data migrates between nodes). This is online but may cause brief latency spikes during migration.

    **Managed services** (recommended unless specific requirements demand self-management):
    - AWS ElastiCache (Redis): Supports Cluster Mode Enabled (Redis Cluster), automatic failover, read replicas, encryption, and backup. Cluster Mode Disabled for Sentinel-equivalent HA.
    - Amazon MemoryDB: Redis-compatible with strong consistency and durability (multi-AZ transaction log). Use when cache data must not be lost (sessions, state).
    - GCP Memorystore: Managed Redis with automatic failover.
    - Azure Cache for Redis: Managed Redis with clustering, geo-replication, and persistence.

    **Multi-region caching**:
    - For globally distributed applications, deploy a cache instance in each region. Options:
      - **Independent caches per region**: Each region has its own cache, populated independently. Simplest, but cache hit rate is lower in each region (each region has its own cold start).
      - **Global replication** (Redis Enterprise Active-Active, MemoryDB multi-region): Data is replicated across regions. Higher hit rate, but replication latency means writes in one region may not be immediately visible in another. Conflict resolution for concurrent writes (last-write-wins or CRDT-based).
      - **Read from local cache, invalidate globally**: Writes invalidate cache entries in all regions via a global event bus (Kafka, SNS/SQS). Each region repopulates from its local data source on the next read.
