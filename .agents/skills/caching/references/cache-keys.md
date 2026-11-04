# Phase 4: Cache Key Design

8. **Design cache key structure.** Cache key design directly affects hit rate, debuggability, and operational management. Poor key design causes: missed cache hits (same data, different key), key collisions (different data, same key), and difficulty in bulk invalidation.

   **Key naming conventions**:
   - Use a hierarchical, human-readable, delimited format: `{service}:{entity}:{identifier}:{variant}`
   - Examples:
     - `catalog:product:prod_abc123` — single product
     - `catalog:product:prod_abc123:details` — product details view
     - `catalog:product:prod_abc123:pricing:usd` — product pricing in USD
     - `orders:customer:cust_xyz:recent:page1` — recent orders for a customer, page 1
     - `search:products:q=shoes:sort=price:page=1` — search results
   - Use `:` as the delimiter (Redis convention, supports namespace scanning with `SCAN MATCH catalog:product:*`).
   - Keep keys as short as practical — each key consumes memory. For Redis, long keys have measurable overhead at high cardinality. But do not sacrifice readability for brevity — debuggability matters.

9. **Include all cache-varying dimensions in the key.** The cache key must uniquely identify the exact data being cached. If two requests should return different data, they must have different cache keys:

   - **Entity identity**: The primary identifier (product ID, user ID, order ID).
   - **Locale / language**: If responses vary by locale, include it: `catalog:product:prod_abc:en-US`.
   - **Currency**: If pricing varies by currency: `pricing:prod_abc:usd`.
   - **User-specific variants**: If the cached data is personalized, include the user ID: `feed:user:usr_123:page1`. **Warning**: Per-user caching has high cardinality and low hit rates — evaluate whether the cache is actually effective.
   - **Pagination**: Include page/cursor/offset in the key.
   - **Query parameters**: For search/filter results, include all relevant query parameters in a deterministic order. Normalize parameters: `sort=price&q=shoes` and `q=shoes&sort=price` must produce the same key. Sort query parameters alphabetically before building the key.
   - **API version**: If cached API responses differ by version: `v2:catalog:product:prod_abc`.
   - **Schema/data version**: Include a version prefix that changes when the cached data structure changes (e.g., after a deployment that adds a field to the cached object): `v3:catalog:product:prod_abc`. This prevents deserialization errors when old-format cached data is read by new code.

10. **Design key generation for computed/query results.** For cached database query results or computed values where the "identity" is a set of parameters:
    - Hash the normalized, deterministic query parameters: `search:products:sha256(canonical_query_string)`.
    - Include enough of the parameters in plaintext for debuggability: `search:products:q=shoes:sort=price:hash=a1b2c3`.
    - For SQL query result caching, hash the full parameterized query + parameter values. Never include raw SQL in the cache key (it is too long and may contain sensitive data).

11. **Design cache key cardinality management.** High-cardinality caches (millions of unique keys) require explicit management:
    - Estimate the total number of unique cache keys: `unique_entity_count × variants_per_entity`.
    - Estimate total memory: `key_count × (average_key_size + average_value_size + Redis_overhead_per_key)`. Redis overhead is approximately 50-100 bytes per key for metadata.
    - If cardinality is too high for available memory, prioritize: cache only the most frequently accessed keys (LRU eviction handles this naturally), reduce the number of variants, or increase the cache size.
    - Monitor cardinality in production: track key count and memory usage trends.
