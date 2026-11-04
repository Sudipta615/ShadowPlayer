### Phase 7: Partitioning and Sharding

24. **Design table partitioning for large tables.** Partitioning improves query performance and operational management for tables that grow continuously. Apply when a single table is projected to exceed 50-100M rows or when data lifecycle management (retention, archival) needs differ by partition.

    - **Range partitioning** (most common): Partition by time range (monthly, weekly, or daily based on ingestion rate). Example: `orders` partitioned by `created_at` month. Queries that filter by time range scan only relevant partitions (partition pruning).
    - **List partitioning**: Partition by a discrete value (tenant_id, region, status). Use when queries almost always filter by the partition key.
    - **Hash partitioning**: Partition by hash of a column for even distribution when there is no natural range or list key. Useful for distributing load but does not enable range-based partition pruning.
    - **Define the partition key**: Must align with the most frequent query filter. If 90% of queries filter by `created_at`, partition by time. If most queries filter by `tenant_id`, partition by tenant.
    - **Define the partition granularity**: Too few partitions (yearly) provide little benefit. Too many partitions (per-minute) cause overhead in planning and metadata management. Choose based on data volume per partition — target 1M-100M rows per partition as a guideline.
    - **Define partition lifecycle**: Automate partition creation (create future partitions ahead of time) and partition detachment/archival (detach old partitions, move to cold storage, or drop after retention period).
    - **Index each partition**: In PostgreSQL declarative partitioning, indexes are created on each partition individually. Unique indexes must include the partition key.

25. **Design database sharding (horizontal scaling across database instances) only when necessary.** Sharding is the strategy of last resort after exhausting:
    - Vertical scaling (bigger instance).
    - Read replicas for read scaling.
    - Table partitioning for data management.
    - Caching for read reduction.
    - CQRS for separating read and write paths.

    If sharding is genuinely required:
    - **Choose the shard key**: Must distribute data evenly (avoid hot shards), must be present in every query (to enable shard routing), and should align with the primary access pattern. Common shard keys: tenant_id (for multi-tenant SaaS), user_id (for user-scoped data), geographic region.
    - **Define the sharding strategy**:
      - **Application-level sharding**: Application determines which shard to query. Most control, most application complexity. Use routing configuration or consistent hashing.
      - **Proxy-based sharding**: A proxy layer (Vitess, Citus, ProxySQL) handles shard routing transparently. Reduces application complexity but introduces infrastructure complexity.
      - **Managed sharding**: Some managed databases handle sharding natively (DynamoDB, Cosmos DB, CockroachDB, TiDB). Prefer these if the technology fits the access patterns.
    - **Address cross-shard queries**: Queries that span shards are expensive and complex. Design the shard key to minimize cross-shard queries. If a query cannot avoid spanning shards, design a separate aggregation layer (materialized views, analytics database).
    - **Address cross-shard transactions**: Distributed transactions across shards are extremely complex. Avoid them — design aggregate boundaries to fit within a single shard. If cross-shard coordination is needed, use saga/event-driven patterns.
    - **Plan resharding**: As data grows, you may need to split shards. Design the sharding scheme to support resharding (consistent hashing, virtual shards mapped to physical shards) and plan the operational procedure.