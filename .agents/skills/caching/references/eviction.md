# Phase 10: Eviction Policy Design

22. **Design the eviction policy.** When the cache reaches its memory limit, it must decide which entries to remove to make room for new ones:

    **Configure `maxmemory`**: Set the maximum memory Redis will use. Leave headroom for Redis overhead (fragmentation, replication buffers): set `maxmemory` to 75-85% of available instance memory.

    **Select the eviction policy** (`maxmemory-policy`):

    - **`allkeys-lru`** (recommended as default): Evict the least recently used key from the entire keyspace. Best general-purpose policy — naturally keeps hot data and evicts cold data. Use when all keys are cache entries and any key can be evicted.
    - **`allkeys-lfu`** (Least Frequently Used): Evict the least frequently used key. Better than LRU when access patterns have stable popularity (some keys are consistently popular, not just recently accessed). Prevents "cache pollution" where a one-time scan of many keys evicts frequently used data.
    - **`volatile-lru`**: Evict the least recently used key only among keys with a TTL set. Keys without TTL are never evicted. Use when mixing cache entries (with TTL) and persistent data (without TTL, e.g., configuration) in the same Redis instance. **Not recommended** — mix cache and persistent data in separate Redis instances/databases.
    - **`volatile-ttl`**: Evict the key with the shortest remaining TTL. Use when you want entries closest to expiration to be evicted first.
    - **`noeviction`**: Do not evict any keys. Return an error on write when memory is full. Use for data stores where data loss is unacceptable (session store, rate limiting state). Requires careful capacity planning.
    - **`allkeys-random`**: Evict a random key. Use only as a baseline for comparison — LRU and LFU are almost always better.

    **Recommendation**: Use `allkeys-lfu` for production caches with stable access patterns. Use `allkeys-lru` if you are unsure of the access distribution or if access patterns are bursty. Use `noeviction` for non-cache use cases (sessions, locks, rate limiting) where eviction would cause incorrect behavior.

    Monitor the eviction rate (`INFO stats` → `evicted_keys`). A sustained high eviction rate indicates the cache is too small for the working set — either increase memory or reduce the cached data volume.
