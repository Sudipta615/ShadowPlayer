# Phase 15: Cache Observability

33. **Design cache monitoring metrics.** Cache health must be continuously monitored. Without monitoring, you cannot know if the cache is providing value, or if it is a liability:

    **Hit rate metrics** (the primary measure of cache effectiveness):
    - **Hit rate** = `cache_hits / (cache_hits + cache_misses) × 100%`.
    - Target: > 90% for most caches. > 95% is excellent. > 99% for highly optimized, stable-data caches.
    - Below 80%: Investigate — the cache may not be providing significant value. Possible causes: too many unique keys (high cardinality, low repetition), TTL too short, eviction rate too high (cache too small), or the access pattern is not cache-friendly.
    - Track hit rate over time — a declining hit rate indicates a problem (data growth, traffic pattern change, configuration issue).
    - Track hit rate per cache key prefix or data type if possible, not just globally. A global 95% hit rate may mask a 30% hit rate for a specific, critical data type.

    **Latency metrics**:
    - Cache read latency: p50, p95, p99. Typically < 1ms within the same AZ. Alert if p99 exceeds 5ms (indicates network issues, slow operations, or large values).
    - Cache write latency: p50, p95, p99. Typically similar to read latency.
    - Cache miss penalty: Latency of the downstream fetch (database query, API call) that occurs on a cache miss. This is the cost that the cache avoids — tracking it demonstrates the cache's value.
    - End-to-end request latency with cache hit vs. cache miss: Demonstrates the user-facing impact of the cache.

    **Memory and capacity metrics**:
    - Memory used vs. `maxmemory`: `INFO memory` → `used_memory` / `maxmemory`. Alert when utilization exceeds 80%.
    - Memory fragmentation ratio: `mem_fragmentation_ratio`. Ideal: 1.0-1.5. Above 1.5 indicates fragmentation (Redis is using more RSS memory than its data requires). Below 1.0 indicates swapping (critical — Redis performance degrades severely when swapping). Alert on fragmentation ratio > 1.5 or < 1.0.
    - Key count: Total number of keys (`DBSIZE`). Track growth over time.
    - Eviction count: `evicted_keys` from `INFO stats`. A non-zero eviction rate means the cache is full and discarding data. If evictions are high, either increase memory or reduce the cached data volume.

    **Connection metrics**:
    - Connected clients: Track against `maxclients`. Alert at 80% of maximum.
    - Rejected connections: Should be zero. Non-zero indicates `maxclients` is reached.
    - Connection rate: Sudden spikes indicate connection leak or misconfigured pool.

    **Replication metrics** (Redis Sentinel/Cluster):
    - Replication lag: Bytes behind or seconds behind the master. Alert if lag exceeds 1MB or 1 second.
    - Connected replicas: Alert if a replica disconnects.
    - Failover events: Log and alert on every failover.

    **Command metrics**:
    - Commands processed per second: `instantaneous_ops_per_sec`. Track trends to understand traffic growth.
    - Slow log: `SLOWLOG GET`. Track commands exceeding the slow threshold (default 10ms). Investigate and optimize slow commands.
    - Expensive commands: Monitor for `KEYS`, `SMEMBERS` on large sets, `SORT` on large lists, `HGETALL` on large hashes — these block Redis and cause latency spikes for all clients.

34. **Design cache dashboards.** Build and maintain:

    **Dashboard 1: Cache Health Overview**
    - Overall hit rate (trending over hours, days).
    - Read/write latency percentiles (p50, p95, p99).
    - Memory utilization vs. limit.
    - Eviction rate.
    - Connected clients.
    - Commands per second.
    - Key count.

    **Dashboard 2: Cache Effectiveness**
    - Hit rate by key prefix / data type.
    - Cache miss penalty (latency of downstream fetches on miss).
    - Estimated database load saved by cache (cache_hits × estimated_db_query_time).
    - Data freshness: time since last invalidation/refresh for critical cached data.

    **Dashboard 3: Cache Infrastructure** (per node/shard)
    - CPU utilization per Redis instance.
    - Memory per node.
    - Network I/O per node.
    - Replication lag per replica.
    - Slow log entries.
    - Cluster slot distribution and migration status.

35. **Design cache alerting.** Define actionable alerts:

    **Critical (page — requires immediate response)**:
    - Cache service unreachable for > 30 seconds.
    - Memory utilization > 95% with `noeviction` policy (writes will fail).
    - Memory fragmentation ratio < 1.0 (swapping — severe performance degradation).
    - Hit rate drops below 50% for > 5 minutes (cache is effectively useless — all traffic hitting the database).
    - Replication lag > 30 seconds (data loss risk on failover).
    - All replicas disconnected from master.

    **Warning (ticket — investigate within business hours)**:
    - Hit rate drops below 80% for > 15 minutes.
    - Memory utilization > 80%.
    - Eviction rate > 100 keys/second sustained for > 10 minutes.
    - Cache read latency p99 > 5ms sustained for > 5 minutes.
    - Slow log entries increasing (> 10 slow commands per minute).
    - Connection count > 70% of `maxclients`.
    - Memory fragmentation ratio > 1.5.

    **Informational (dashboard/log)**:
    - Hit rate trends (weekly comparison).
    - Key count growth trends.
    - Command distribution changes (new command patterns appearing).

    Every critical alert must have a documented runbook.
