# Phase 2: Cache Layer Selection

4. **Select the cache layer(s).** Different caching layers solve different problems. Select based on the specific requirements from Phase 1:

   **Layer 1: In-process (application-level) cache**
   - **What it is**: A data structure (hash map, LRU cache) within the application process's memory. No network hop.
   - **When to use**: Small reference data that changes infrequently and is accessed on nearly every request (feature flags, configuration, country/currency lookup tables, compiled templates, schema metadata). Data where < 1µs access time matters. Per-request memoization of repeated computations within a single request lifecycle.
   - **Advantages**: Fastest possible access (no network, no serialization). No external dependency.
   - **Disadvantages**: Not shared across application instances — each instance has its own copy. Inconsistency between instances during updates (instance A has the new value, instance B still has the old value until its TTL expires or it is restarted). Consumes application heap memory — can cause garbage collection pressure or OOM if sized incorrectly. Data is lost on process restart.
   - **Size constraint**: Keep the in-process cache small (tens of MB, not GB). If you need to cache more, use a distributed cache.
   - **Implementation**: Language-native caches (Go: `sync.Map` or `groupcache`; Java: Caffeine, Guava Cache; Python: `functools.lru_cache`, `cachetools`; Node.js: `node-cache`, `lru-cache`).
   - **Invalidation**: TTL-based (simplest — each entry expires after a fixed duration), or event-based (subscribe to a pub/sub channel for invalidation signals). For multi-instance consistency, broadcast invalidation events via Redis pub/sub, Kafka, or a similar mechanism.

   **Layer 2: Distributed cache (shared across application instances)**
   - **What it is**: A separate caching service (Redis, Memcached, Valkey) accessible by all application instances over the network.
   - **When to use**: Data that must be consistent across all application instances, data that is too large for in-process caches, data that must survive application restarts, session data, rate limiting counters, cached query results, cached API responses, cached computations.
   - **Advantages**: Shared state across all instances. Survives application restarts (if the cache service is persistent). Can scale independently of the application.
   - **Disadvantages**: Network round-trip per access (typically 0.5-2ms within the same availability zone). Requires serialization/deserialization. Adds an operational dependency. Cache service itself can fail.

   **Redis** (recommended as the default distributed cache):
   - Rich data structure support: strings, hashes, lists, sets, sorted sets, streams, bitmaps, HyperLogLog. These enable use cases beyond simple key-value caching (leaderboards with sorted sets, rate limiting with sorted sets or token bucket scripts, pub/sub for cache invalidation broadcasts, distributed locks with SETNX).
   - Lua scripting for atomic multi-step operations.
   - Persistence options: RDB (snapshots) and AOF (append-only file) for durability. Can function as a primary data store for ephemeral or reconstructable data.
   - Clustering: Redis Cluster for horizontal scaling and high availability. Redis Sentinel for HA without sharding.
   - Pub/sub for cache invalidation signaling.
   - When to choose: Most caching scenarios. When you need data structures beyond simple key-value. When you need persistence. When you need pub/sub. When you need Lua scripting for atomic operations.

   **Memcached** (specific use cases only):
   - Pure key-value store. Multi-threaded (uses all CPU cores per instance, unlike single-threaded Redis per shard).
   - No persistence, no data structures, no scripting, no pub/sub, no clustering (client-side sharding via consistent hashing).
   - When to choose: Simple key-value caching with very large datasets where Redis's memory overhead per key (due to data structure metadata) is a concern. When you need multi-threaded performance on a single large instance. When you are already operating Memcached and the requirements are met.
   - When NOT to choose: When you need any functionality beyond get/set/delete. When you need persistence. When you need server-side sharding. Default to Redis in new designs.

   **Valkey** (Redis fork, open-source):
   - API-compatible with Redis. Choose Valkey if open-source licensing is important (Redis changed to a non-open-source license in 2024). Feature-equivalent for most caching use cases.

   **Managed services**: AWS ElastiCache (Redis/Memcached), Amazon MemoryDB (durable Redis-compatible), GCP Memorystore, Azure Cache for Redis. Recommend managed services unless there is a specific reason to self-manage (cost at extreme scale, compliance, customization).

   **Layer 3: CDN / Edge cache**
   - **What it is**: A globally distributed cache at the network edge, close to end users. Cloudflare, AWS CloudFront, Fastly, GCP Cloud CDN, Akamai.
   - **When to use**: Static assets (images, CSS, JS, fonts), publicly cacheable API responses (product catalog for unauthenticated users, public content), any response where reducing latency to the end user by serving from a nearby edge node is valuable.
   - **Advantages**: Dramatically reduces latency for geographically distributed users. Offloads traffic from origin servers. Built-in DDoS absorption.
   - **Disadvantages**: Limited invalidation capabilities (purging is eventual, not instant). Difficult to cache personalized or authenticated responses (requires cache key segmentation by auth state or Vary headers). Cache behavior is controlled by HTTP headers — the application must set correct caching headers.
   - **When NOT to use**: For user-specific data, real-time data, or data that must be consistent within seconds of changes (unless using edge computing with origin pulls and short TTLs).

   **Layer 4: Database-level cache**
   - **Query result cache** (MySQL query cache — deprecated and removed in MySQL 8.0; PostgreSQL has no built-in query cache): Generally not recommended as a caching strategy. Database-level caching is better addressed by proper buffer pool sizing (shared_buffers in PostgreSQL) and OS file system cache.
   - **Materialized views**: Precomputed query results stored in the database. Useful for expensive aggregations. Not a "cache" in the traditional sense but serves a similar purpose. Refresh must be triggered explicitly (`REFRESH MATERIALIZED VIEW CONCURRENTLY`).
   - **PostgreSQL shared_buffers / InnoDB buffer pool**: The database's internal page cache. This is not a caching "decision" — it is a configuration tuning task (covered by the database-performance skill). Ensure it is properly sized before adding external caching layers.

   **Layer 5: HTTP response cache (client-side / browser)**
   - **What it is**: Cache controlled by HTTP headers (`Cache-Control`, `ETag`, `Last-Modified`). Stored in the browser or HTTP client.
   - **When to use**: API responses that are safe to cache on the client (public data, user-specific data with appropriate `private` directive). Reduces server load and network round-trips entirely — the client never sends the request.
   - **Design**: Covered in detail in step 24 (HTTP Caching).

   **Multi-level caching (L1 + L2)**:
   When combining layers (e.g., in-process L1 + distributed L2), define the interaction:
   - Request checks L1 (in-process) → hit → return. Miss → check L2 (distributed) → hit → populate L1, return. Miss → fetch from source → populate L2, populate L1, return.
   - L1 TTL must be shorter than L2 TTL to limit cross-instance inconsistency. Example: L1 TTL = 30 seconds, L2 TTL = 5 minutes.
   - Invalidation must propagate to both layers. If an event invalidates L2, it must also invalidate L1 on all instances (via pub/sub broadcast).
   - Multi-level caching adds complexity. Use only when the latency difference between L1 and L2 justifies the additional invalidation complexity and memory cost.

5. **Justify the selection.** For each caching layer chosen, state:
   - Which data / access patterns from the catalog (step 2) this layer caches.
   - Why this layer is appropriate (access latency, data size, sharing requirements, consistency needs).
   - What the layer costs (operational complexity, memory cost, invalidation complexity, failure mode).
   - What alternative was considered and why it was rejected.
