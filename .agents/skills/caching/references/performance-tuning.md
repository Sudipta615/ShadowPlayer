# Phase 16: Cache Performance Tuning

36. **Tune Redis performance.** For Redis (the most common distributed cache), apply these configurations:

    **Connection management**:
    - `maxclients`: Set based on expected client count with headroom. Default: 10000. Ensure the OS `ulimit` for file descriptors is set higher.
    - Client-side connection pooling: Every application instance must use a connection pool (see database-performance skill principles). Pool size per instance: 5-20 connections for most applications. One connection can handle many commands via pipelining.
    - `timeout`: Set an idle connection timeout (e.g., 300 seconds) to reclaim connections from crashed or misconfigured clients.

    **Persistence tuning** (if persistence is enabled):
    - RDB: `save` intervals. For caches, less frequent saves are fine: `save 900 1` (save after 900 seconds if at least 1 key changed). Reduce to avoid I/O spikes on large datasets.
    - AOF: `appendfsync everysec` (recommended — fsync once per second, max 1 second of data loss on crash). `appendfsync always` (fsync every write — safest but slowest). `appendfsync no` (OS decides when to fsync — fastest, most data loss risk).
    - For pure caches where data can be reloaded from the source: disable persistence entirely (`save ""`, `appendonly no`) to maximize performance and eliminate I/O overhead.

    **Command optimization**:
    - **Use pipelining**: Batch multiple Redis commands into a single round-trip. Instead of 10 sequential `GET` commands (10 round-trips), pipeline them (1 round-trip). Reduces network overhead dramatically.
    - **Use `MGET`/`MSET`** for multiple key operations instead of individual `GET`/`SET` commands.
    - **Avoid blocking commands in production**: `KEYS *` (blocks Redis, use `SCAN` instead), `FLUSHALL`, `FLUSHDB` (use `UNLINK` for non-blocking delete), `DEBUG`, `SORT` on large datasets.
    - **Use `UNLINK` instead of `DEL`** for large values (> 1KB): `UNLINK` reclaims memory asynchronously in a background thread, while `DEL` blocks the main thread.
    - **Use Lua scripts** for atomic multi-step operations (check-and-set, conditional update, rate limiting) instead of multi-command sequences with `WATCH`/`MULTI`/`EXEC`. Lua scripts execute atomically on the server, eliminating race conditions.

    **Memory configuration**:
    - `maxmemory`: As discussed in step 22.
    - `maxmemory-policy`: As discussed in step 22.
    - `lazyfree-lazy-eviction yes`: Evict keys asynchronously in a background thread (prevents eviction from blocking the main thread).
    - `lazyfree-lazy-expire yes`: Expire keys asynchronously.
    - `lazyfree-lazy-server-del yes`: Background thread handles `DEL` of large keys internally.

37. **Tune application-level cache performance.**
    - **Measure serialization overhead**: Profile the time spent serializing and deserializing cached values. If serialization is a significant fraction of the total cache access time, switch to a faster format (MessagePack, Protocol Buffers) or cache pre-serialized responses.
    - **Measure network overhead**: The round-trip to the cache should be < 1ms within the same AZ. If it is higher, check: network configuration, instance placement (same AZ?), connection pooling, and TLS overhead (TLS adds ~0.5ms per connection establishment; use connection pooling to amortize).
    - **Pipeline and batch requests**: As discussed in step 36. Measure the impact of pipelining — it typically provides 3-10x throughput improvement.
    - **Avoid cache-on-read for every request**: Not every database query needs to be cached. Cache only queries that are: frequently repeated, expensive to execute, and tolerant of staleness. Caching cheap, infrequent queries adds complexity without meaningful benefit.
