# Phase 7: Cache Failure Handling and Resilience

15. **Design cache stampede prevention.** A cache stampede (thundering herd) occurs when a popular cache entry expires and many concurrent requests simultaneously hit the data source to repopulate it. At scale, this can overload the database and cause cascading failures.

    **Prevention mechanisms**:

    **Mechanism 1: Locking (mutex-based repopulation)**:
    - On cache miss, the first request acquires a lock (Redis `SETNX` with TTL) and repopulates the cache. Concurrent requests either wait for the lock to release and then read from cache, or return a stale value if available.
    - Implementation:
      ```
      value = cache.get(key)
      if value is not None:
          return value
      if cache.set(lock_key, "1", nx=True, ex=30):  # Acquire lock
          try:
              value = fetch_from_source()
              cache.set(key, value, ex=ttl)
          finally:
              cache.delete(lock_key)
          return value
      else:
          # Another request is repopulating — wait briefly and retry
          sleep(50ms)
          return cache.get(key) or fetch_from_source()  # Fallback if lock holder fails
      ```
    - Lock TTL must be longer than the expected source fetch time but short enough to unblock waiters if the lock holder crashes.

    **Mechanism 2: Probabilistic early expiration**:
    - Each request that accesses a cache entry has a small probability of refreshing it before the TTL expires. The probability increases as the entry approaches its expiry.
    - Formula: `should_refresh = random() < (time_since_set / ttl) ^ beta` (where beta controls aggressiveness).
    - Advantage: No locks, no coordination. Spreads refresh load naturally.
    - Disadvantage: Non-deterministic. In rare cases, the entry may still expire without being refreshed.

    **Mechanism 3: Background refresh (refresh-ahead)**:
    - A background process or thread refreshes cache entries before they expire. The cache is never empty for popular keys.
    - Implementation: Track access recency. For keys accessed within the last refresh window, proactively refresh when TTL reaches a threshold (e.g., < 20% remaining).
    - Advantage: Cache consumers never experience a miss on popular keys.
    - Disadvantage: Requires background infrastructure. May refresh keys that are no longer being accessed (wasted work).

    **Mechanism 4: Stale-while-revalidate**:
    - Serve the expired (stale) cache entry to the caller while triggering an asynchronous background refresh.
    - The caller gets an immediate response (stale but fast). The next caller gets the fresh value.
    - Implementation: Store both the value and its expiry timestamp. On access, if expired, return the stale value and trigger async refresh. Set a maximum stale duration beyond which even stale data is not served.
    - Advantage: Zero cache-miss latency for the caller. Eliminates stampede.
    - Disadvantage: Callers may see briefly stale data during the refresh window.

    Choose the mechanism based on the use case. For most systems, locking + stale-while-revalidate provides the best balance. State the choice and rationale.

16. **Design cache penetration prevention.** Cache penetration occurs when requests repeatedly ask for data that does not exist in the data source. Every request is a cache miss and hits the database, because there is nothing to cache.

    - **Null object caching**: When the data source returns no result, cache a sentinel "not found" value with a short TTL (30-60 seconds): `cache.set("product:nonexistent_id", NULL_SENTINEL, ex=60)`. On cache hit with the sentinel, return "not found" without hitting the database. This prevents repeated database queries for non-existent keys.
    - **Bloom filter**: For very high-cardinality datasets where many lookups target non-existent keys, use a Bloom filter in front of the cache/database. If the Bloom filter says the key definitely does not exist, skip the database query entirely. If it says "maybe exists," proceed with the normal cache/database lookup. Bloom filters have a small false-positive rate (tunable) and zero false-negative rate. Rebuild periodically as data changes.
    - **Input validation**: Validate that the requested key/ID is in a valid format before querying. Reject obviously invalid keys (wrong format, wrong length) at the API layer.

17. **Design cache avalanche prevention.** A cache avalanche occurs when a large number of cache entries expire simultaneously (e.g., after a mass cache warming at startup, or after a cache server restart), causing a sudden spike in database load.

    - **TTL jitter** (primary prevention): Add random jitter to TTLs so entries expire at different times (see step 12).
    - **Staggered cache warming**: When warming the cache (step 19), load entries gradually rather than all at once.
    - **Circuit breaker on the data source**: If the database or upstream service is overwhelmed by cache miss traffic, use a circuit breaker to reject excess requests rather than cascading the overload. Return errors or degraded responses rather than killing the database.
    - **Multi-layer caching**: If the L2 (distributed) cache fails, the L1 (in-process) cache provides partial coverage while the L2 recovers.

18. **Design cache failure handling.** The cache is an optimization, not a correctness requirement (unless you are using write-behind, which is explicitly a correctness concern). Design the system to function without the cache:

    **Cache-as-optimization principle**: If the cache is unavailable, the system should continue to function correctly, though with degraded performance (higher latency, higher database load).

    **Implementation**:
    - **Wrap all cache operations in try/catch** (or equivalent error handling). A cache timeout or connection error must never cause the request to fail — it should fall through to the data source.
    - **Cache timeouts**: Set aggressive timeouts on cache operations:
      - Connection timeout: 100-500ms.
      - Read/write timeout: 50-200ms.
      - If the cache does not respond within this window, skip it and go to the data source. A slow cache is worse than no cache — it adds latency without providing value.
    - **Circuit breaker on the cache**: If the cache fails repeatedly (e.g., 5 consecutive failures within 10 seconds), open the circuit breaker and stop attempting cache operations for a cooldown period (30-60 seconds). This prevents every request from paying the cache timeout penalty during an outage. After the cooldown, half-open the circuit (try one request) and close the circuit if it succeeds.
    - **Graceful degradation**: When the cache is down:
      - Increase database connection pool size temporarily (if possible) to handle the increased load.
      - Enable application-level rate limiting on cache-dependent endpoints to prevent database overload.
      - Log the cache failure and alert the operations team.
      - Consider serving stale data from a backup source (replicated cache, database query cache) if available.
    - **Cache recovery**: When the cache comes back online after a failure:
      - Allow the cache to warm naturally (cache-aside populates on each miss). Do not attempt to warm the entire cache at once — this can overload the database.
      - Or trigger a controlled cache warming process (step 19) that loads critical data gradually.

    **When the cache IS a correctness dependency** (rate limiting, distributed locks, session storage):
    - These use cases require the cache to be highly available. Design for HA:
      - Redis Sentinel or Redis Cluster with automatic failover.
      - Multi-AZ deployment with synchronous replication for the session/lock data.
      - Fallback mechanism: if Redis is down for rate limiting, fall back to in-memory rate limiting per instance (less accurate but still provides some protection).
      - For sessions: fall back to signed JWT tokens (no server-side state needed) during cache outage, or use database-backed sessions as a degraded fallback.
