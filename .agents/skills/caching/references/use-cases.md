# Phase 13: Caching for Specific Use Cases

27. **Design API response caching.** For caching entire API endpoint responses:
    - **Cache the serialized response** (JSON/MessagePack) rather than the deserialized object. This avoids re-serialization on cache hit.
    - **Include all response-varying parameters in the cache key** (see step 9): URL path, query parameters (normalized), and relevant headers (locale, API version).
    - **Cache at the right layer**:
      - CDN/edge cache for public responses (highest performance, lowest origin load).
      - API gateway cache for authenticated but reusable responses (e.g., all users in the same organization see the same dashboard).
      - Application-level cache for complex, computed responses that require application logic to determine cacheability.
    - **Respect the HTTP caching contract**: Set `Cache-Control`, `ETag`, and `Vary` headers correctly (steps 24-26) so that intermediaries (CDNs, proxies, browsers) can cache effectively.

28. **Design query result caching.** For caching database query results:
    - **Cache the query result**, not individual rows. A query result is a specific projection (selected columns, ordering, filtering) of the data.
    - **Cache key**: Derive from the query and its parameters. For parameterized queries: `query_cache:{query_hash}:{param_hash}`. For known, named queries: `orders:customer:cust_123:status:active:page:1`.
    - **Invalidation**: Invalidate when any data that contributes to the result changes. For simple single-entity queries, invalidate by entity ID. For complex queries (joins, aggregations), invalidation is harder:
      - Use event-driven invalidation: when any contributing entity changes, invalidate all cached queries that might include it. This may over-invalidate (invalidating queries that are not actually affected), but it prevents serving stale results.
      - Use coarse-grained invalidation: invalidate all cached queries for a table when any row in that table changes. Simple but wasteful — acceptable when write frequency is low.
      - Accept TTL-based consistency: for complex queries, accept that the cache may be stale for up to the TTL duration. This is often the most practical approach for reporting/analytics queries.

29. **Design session caching.** For storing user sessions in a cache:
    - Use Redis with persistence (AOF or RDB) to survive restarts without losing sessions.
    - Session key: `session:{session_id}`. Session ID is a CSPRNG-generated opaque token (see authentication skill).
    - Session value: Serialized session data (user ID, roles, session metadata). Keep session data small (< 1KB). Store large data (shopping cart contents, form drafts) separately with references in the session.
    - TTL: Set to the session absolute timeout (e.g., 24 hours). Update TTL on each access to implement idle timeout (`EXPIRE` command).
    - **Eviction policy**: Use `noeviction` for the Redis instance storing sessions. Evicting an active session logs the user out unexpectedly. Size the Redis instance to hold all active sessions with headroom.
    - **Security**: Do not store sensitive data in the session (passwords, full credit card numbers). If sessions must be encrypted at rest, encrypt the session value before storing in Redis.

30. **Design rate limiting with cache.** Redis is commonly used as the backing store for rate limiting:
    - **Fixed window**: Increment a counter keyed by `ratelimit:{client_id}:{window}` (e.g., `ratelimit:api_key_123:202401151030`). Set TTL to the window duration. Check counter against limit.
    - **Sliding window log**: Store timestamps of each request in a sorted set. Count entries within the window using `ZRANGEBYSCORE`. Remove entries outside the window using `ZREMRANGEBYSCORE`. More accurate than fixed window but uses more memory.
    - **Token bucket** (recommended for most rate limiting): Implement via Lua script for atomicity:
      - Check the bucket: how many tokens are available?
      - Calculate tokens added since last check based on elapsed time and refill rate.
      - If tokens are available, decrement and allow. If not, reject.
      - Advantages: Allows short bursts while maintaining a long-term average rate.
    - **Sliding window counter** (good balance of accuracy and efficiency): Combine the current window's counter with the previous window's counter, weighted by the elapsed time in the current window. More accurate than fixed window, less memory than sliding log.
    - Store rate limiting state in Redis with `noeviction` policy. Rate limiting data must not be evicted — an evicted rate limit counter resets the limit, allowing an abuse burst.

31. **Design computed value caching.** For caching the results of expensive computations:
    - **ML model inference results**: Cache predictions keyed by the input feature hash. Useful when the same input is queried repeatedly (product recommendations for popular items, classification of commonly submitted content).
    - **Aggregation results**: Cache the result of expensive aggregation queries (daily sales totals, user activity summaries). Refresh on a schedule or via event-driven computation. Accept staleness equal to the refresh interval.
    - **Rendered content**: Cache rendered templates, generated PDFs, processed images. Key by the input parameters + template version. Invalidate when the template or input data changes.
    - **Configuration compilation**: Cache compiled/resolved configuration (feature flags, A/B test assignments, routing rules). Refresh via event on configuration change.
