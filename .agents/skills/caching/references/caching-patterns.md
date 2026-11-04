# Phase 3: Caching Pattern Selection

6. **Select the caching pattern for each cached data type.** The caching pattern defines how data flows between the application, cache, and data source. Choosing the wrong pattern causes stale data, cache misses, or data loss.

   **Cache-Aside (Lazy Loading)** — recommended as the default pattern:
   - **Flow**:
     1. Application receives a read request.
     2. Application checks the cache for the requested key.
     3. **Cache hit**: Return the cached value directly.
     4. **Cache miss**: Application queries the data source (database, upstream service).
     5. Application writes the result to the cache with a TTL.
     6. Application returns the result to the caller.
   - **Write path**: Application writes to the data source. Optionally invalidates or updates the cache entry.
   - **Advantages**: Simple to implement. The application controls all cache interactions. Cache only contains data that has been requested (no wasted memory on unaccessed data). Cache failure does not prevent the application from functioning — it degrades to hitting the data source directly.
   - **Disadvantages**: Cache miss penalty — the first request for any key pays the full latency of the data source. Potential for stale data if the data source is updated without cache invalidation. Risk of cache stampede (see step 15).
   - **When to use**: Most read-heavy workloads. When you want full control over caching logic. When the data source is the system of record and the cache is purely an optimization.

   **Read-Through**:
   - **Flow**: The application interacts only with the cache. On a cache miss, the cache itself loads the data from the data source, stores it, and returns it.
   - **Advantages**: Application code is simpler — it only reads from the cache, never from the data source directly. Cache loading logic is centralized in the cache layer.
   - **Disadvantages**: Requires the cache layer to know how to load data from the source (more complex cache configuration or a cache-as-a-service with loader support). Same cache miss penalty as cache-aside.
   - **When to use**: When using a cache library or framework that supports loaders (Caffeine with `LoadingCache` in Java, Guava `CacheLoader`). When you want to centralize data loading logic.

   **Write-Through**:
   - **Flow**: When the application writes data, it writes to the cache and the cache synchronously writes to the data source. The cache always has the latest data.
   - **Advantages**: Cache is always consistent with the data source (no stale reads after writes). No separate invalidation needed.
   - **Disadvantages**: Every write incurs the latency of both cache and data source writes (write latency increases). Writes must go through the cache layer, coupling the write path to the cache. If the cache is down, writes fail (unless you implement a fallback to write directly to the data source, which can cause cache inconsistency).
   - **When to use**: When read-after-write consistency is critical and the write volume is low. When the cache is a controlled abstraction layer in front of the data source.

   **Write-Behind (Write-Back)**:
   - **Flow**: The application writes to the cache. The cache asynchronously writes to the data source after a delay or when a batch threshold is reached.
   - **Advantages**: Very fast writes (only cache latency). Batches writes to the data source, reducing load. Absorbs write spikes.
   - **Disadvantages**: **Risk of data loss** — if the cache crashes before the asynchronous write completes, data is lost. Data source is temporarily behind the cache (eventual consistency). Complex to implement correctly with error handling and retry logic.
   - **When to use**: Only when write performance is critical AND the data can tolerate potential loss (analytics events, non-critical counters, session data that can be reconstructed). Never for financial data, user-generated content, or any data where loss causes business impact.
   - **Always state the data loss risk explicitly when recommending write-behind.**

   **Refresh-Ahead (Predictive Refresh)**:
   - **Flow**: The cache proactively refreshes entries before they expire, based on the entry's remaining TTL and recent access frequency. When a cache entry is accessed and its remaining TTL is below a threshold (e.g., < 20% of the original TTL), the cache triggers an asynchronous background refresh.
   - **Advantages**: Eliminates cache miss latency for frequently accessed keys — the cache always has a fresh value ready. Reduces cache stampede risk.
   - **Disadvantages**: Requires background refresh infrastructure. Refreshes data that may not be requested again (wasted work). More complex to implement.
   - **When to use**: For high-traffic, latency-sensitive data where even occasional cache miss latency is unacceptable. For data with predictable access patterns.

   For each data type in the cache catalog (step 2), state which pattern is used and why.

7. **Design the write-path cache interaction.** When source data is modified, how does the cache handle it? This decision directly controls consistency:

   **Option A: Invalidate on write (delete the cache entry)** — recommended as default:
   - When data is written to the source, delete the corresponding cache entry.
   - The next read will be a cache miss, triggering a fresh load from the source.
   - Advantages: Simple. Guarantees the next read gets fresh data. Avoids race conditions between concurrent writes and cache updates.
   - Disadvantages: The next reader pays the cache miss penalty.
   - Preferred when: writes are infrequent relative to reads, cache miss latency is acceptable, and simplicity is valued.

   **Option B: Update on write (write new value to cache)**:
   - When data is written to the source, also write the new value to the cache.
   - Advantages: No cache miss after write — subsequent reads get the fresh value immediately.
   - Disadvantages: Race condition risk — if two concurrent writes occur, the cache may end up with the value from the first write while the database has the value from the second write (write 1 updates DB, write 2 updates DB, write 2 updates cache, write 1 updates cache — cache now has stale value from write 1). Mitigate with: conditional cache updates using version numbers, or by accepting that TTL will eventually correct it.
   - Preferred when: read-after-write consistency is important and writes are serialized or the race condition window is acceptable.

   **Option C: Invalidate on write + read-through repopulation**:
   - Invalidate on write, but the next reader triggers a read-through that repopulates the cache.
   - Combines the simplicity of invalidation with the automation of read-through.

   **Important order of operations**: When using cache-aside with invalidate-on-write:
   - **Write the database first, then invalidate the cache.** Never invalidate the cache first — if the database write fails after cache invalidation, the next read will repopulate the cache with the old value (before the write), and the system is consistent. If you invalidate first and the DB write succeeds, the next read correctly loads the new value.
   - However, if you write the DB first and the cache invalidation fails, the cache serves stale data until TTL expiry. Mitigate with: retry the invalidation, or accept the TTL as the staleness bound.
   - **Never update the cache and the database in a non-atomic operation without a defined consistency strategy.** This is the fundamental challenge of caching. State the strategy explicitly.
