# Phase 9: Serialization and Memory Optimization

20. **Design cache value serialization.** The serialization format affects cache performance (serialization/deserialization speed), memory usage (serialized size), and compatibility (schema evolution):

    **JSON** (default recommendation for most cases):
    - Human-readable (aids debugging — you can inspect cached values with `redis-cli GET`).
    - Widely supported across all languages.
    - Disadvantages: Larger than binary formats (field names are repeated in every value), slower to serialize/deserialize than binary formats.
    - When to use: Most applications. When debuggability is valued. When the performance difference between JSON and binary is not measurable in the context of the application's overall latency.

    **MessagePack** (recommended for performance-sensitive caches):
    - Binary format, structurally similar to JSON but more compact (~30-50% smaller). Faster to serialize/deserialize than JSON.
    - Disadvantages: Not human-readable. Requires MessagePack libraries in all consuming languages.
    - When to use: High-throughput caches where serialization overhead or memory usage is measurable. When cache values are large.

    **Protocol Buffers** (recommended for strongly-typed, schema-evolving caches):
    - Strongly typed, schema-defined, very compact, very fast.
    - Supports schema evolution (adding/removing fields without breaking existing cached data).
    - Disadvantages: Requires `.proto` schema definitions and code generation. Not human-readable. More setup overhead.
    - When to use: When cached data has a well-defined, evolving schema shared across multiple services. When cache size and serialization speed are critical.

    **Native language serialization** (Java Serializable, Python pickle, etc.):
    - **Never use for distributed caches.** Security risk (deserialization attacks), not cross-language compatible, fragile across code versions (class changes break deserialization). Acceptable only for in-process L1 caches within the same application where objects are stored directly in memory.

    **Compression** (for large cached values):
    - If cached values are large (> 1KB), apply compression before storing: gzip, LZ4, Snappy, or zstd.
    - LZ4 or Snappy: Fast compression/decompression, moderate compression ratio. Recommended for latency-sensitive caches.
    - gzip or zstd: Higher compression ratio, slower. Recommended when memory savings outweigh the CPU cost.
    - Only compress if the values are large enough for compression to be meaningful. Compressing 100-byte values adds overhead without significant size reduction.
    - Measure compression ratio and CPU impact before committing to compression in production.

21. **Design memory optimization for the cache.** Cache memory is finite and expensive. Optimize usage:

    **Right-size cached values**:
    - Cache only the fields that consumers need, not the entire database row or object. If an endpoint only uses `{id, name, price}`, don't cache `{id, name, price, description, full_spec, images, reviews, ...}`.
    - Use separate cache entries for different access patterns: `product:prod_abc:summary` (lightweight, for listing pages) and `product:prod_abc:full` (complete, for detail pages).

    **Redis-specific memory optimization**:
    - Use the appropriate Redis data structure:
      - **Strings**: For simple key-value pairs. Each string key has ~50 bytes of overhead.
      - **Hashes**: For objects with multiple fields. For small hashes (< `hash-max-ziplist-entries` and `hash-max-ziplist-value` thresholds), Redis uses a memory-efficient ziplist encoding. Store object fields as hash fields: `HSET product:prod_abc name "Shoes" price "49.99"`. More memory-efficient than storing a serialized JSON string for small objects.
      - **Sets and Sorted Sets**: For collections and ranked data. Use when the access pattern requires set operations (membership check, intersection, union, ranked retrieval).
    - Configure `maxmemory` and `maxmemory-policy` (see step 22).
    - Monitor memory usage: `INFO memory`, `MEMORY USAGE key`, `MEMORY DOCTOR`.
    - Identify large keys: `redis-cli --bigkeys` or `MEMORY USAGE` for specific keys. Large keys (> 1MB) cause latency spikes during serialization and network transfer. Break them into smaller keys or compress them.
