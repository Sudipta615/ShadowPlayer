# Phase 17: Capacity Planning

38. **Perform cache capacity planning.** Size the cache infrastructure based on current and projected requirements:

    **Memory capacity**:
    - Calculate: `total_memory = key_count × (avg_key_size + avg_value_size + per_key_overhead)`.
    - Redis per-key overhead: approximately 50-100 bytes depending on data structure type.
    - Add 25-40% overhead for: Redis internal fragmentation, replication buffers, Lua script memory, client output buffers, and operating headroom.
    - Example: 1 million keys × (50 bytes key + 500 bytes value + 80 bytes overhead) = ~630 MB raw. With 30% overhead = ~820 MB. Round up to 1 GB `maxmemory`.
    - Project growth: If key count grows 10% per month, plan for 12-month capacity: 1 million × 1.1^12 = ~3.1 million keys = ~2.6 GB. Provision accordingly or plan a scaling action.

    **Throughput capacity**:
    - Redis single-threaded: A single Redis instance can handle 100,000-300,000 operations/second depending on operation type and value size (simple GET/SET with small values at the high end, complex commands with large values at the low end).
    - If projected throughput exceeds single-instance capacity, plan for Redis Cluster (horizontal scaling) or read replicas (read scaling).
    - Calculate: `peak_ops_per_second = (peak_requests_per_second × cache_operations_per_request)`. If each API request makes 3 cache operations, and peak traffic is 10,000 req/s, the cache sees 30,000 ops/s.

    **Network capacity**:
    - Calculate: `peak_bandwidth = peak_ops_per_second × avg_response_size`.
    - Example: 30,000 ops/s × 1 KB avg = 30 MB/s. Ensure the network interface and Redis instance's network allocation support this.

    **Connection capacity**:
    - Calculate: `max_connections = app_instances × pool_size_per_instance + monitoring_connections + admin_connections`.
    - Ensure `maxclients` accommodates this with 20% headroom.
