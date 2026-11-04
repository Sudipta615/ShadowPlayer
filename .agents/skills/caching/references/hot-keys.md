# Phase 14: Hot Key Management

32. **Identify and mitigate hot keys.** A hot key is a cache key that receives disproportionately high traffic. In a distributed cache, the shard holding the hot key becomes a bottleneck while other shards are idle:

    **Identifying hot keys**:
    - Redis: `redis-cli --hotkeys` (requires `maxmemory-policy` to include LFU), `OBJECT FREQ key`, or monitor `SLOWLOG` for frequently accessed keys.
    - Application-level: Log cache key access counts per interval. Alert when a single key exceeds a threshold (e.g., > 1% of total traffic).
    - Common hot keys: Global configuration, popular product pages, viral content, trending topics, authentication tokens for system-wide service accounts.

    **Mitigation strategies**:

    **Strategy 1: Local (L1) caching of hot keys**:
    - Cache the hot key's value in-process (L1 cache) with a very short TTL (1-5 seconds). Most requests are served from L1 without hitting the distributed cache.
    - The distributed cache (L2) remains the source, but traffic to it is reduced by 95%+.

    **Strategy 2: Key replication / read replicas**:
    - Create multiple copies of the hot key with a suffix: `product:hot_item:{shard_1}`, `product:hot_item:{shard_2}`, ... up to N replicas. Distribute reads randomly across replicas.
    - Write updates to all replicas (fan-out on write).
    - Reduces load on any single shard by N×.
    - Complexity: Maintaining N replicas and ensuring consistency on writes.

    **Strategy 3: Application-level sharding of the value**:
    - If the hot key is a large value (e.g., a leaderboard or a large set), split it into multiple keys by range or hash. Aggregate on read.

    **Strategy 4: Rate limiting at the application level**:
    - If the hot key access is caused by a burst of identical requests (e.g., product page goes viral), collapse identical concurrent requests at the application level: the first request fetches the data, concurrent requests wait for the first one to complete and share the result (request coalescing / singleflight pattern).
    - Go: `golang.org/x/sync/singleflight`. Java: custom implementation with `CompletableFuture`. Node.js: custom implementation with shared `Promise`.
