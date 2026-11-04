# Phase 5: Cache Invalidation Design

12. **Design the invalidation strategy.** Cache invalidation is the hardest problem in caching. An invalidation strategy that is too aggressive wastes cache capacity (low hit rate). An invalidation strategy that is too lax serves stale data (consistency violations). Design the strategy explicitly for each cached data type.

    **Strategy 1: TTL-based expiration (time-to-live)** — the foundation of all cache invalidation:
    - Every cache entry must have a TTL. No entry should live indefinitely. Even if another invalidation mechanism is used (event-based), TTL is the safety net that prevents permanently stale data.
    - **Setting TTL values**:
      - The TTL should be based on the data's update frequency and the acceptable staleness:
        - Data changes every few seconds (real-time metrics): TTL = 5-15 seconds, or do not cache.
        - Data changes every few minutes (order status): TTL = 30-60 seconds.
        - Data changes a few times per day (product catalog): TTL = 5-15 minutes.
        - Data changes rarely (country codes, feature flags, configuration): TTL = 1-24 hours.
        - Data never changes (historical records, immutable events): TTL = 24 hours or longer (still set a TTL for memory management).
      - TTL is a contract with the consumer: "This data may be up to [TTL] seconds stale." Make this contract explicit and ensure stakeholders accept it.
    - **TTL jitter**: When many cache entries are created at the same time (application startup, cache warming), they all expire at the same time, causing a "thundering herd" of cache misses. Add random jitter to TTLs: `actual_ttl = base_ttl + random(0, base_ttl * 0.1)`. This spreads expiration across a time window.

    **Strategy 2: Event-driven invalidation** — for consistency-sensitive data:
    - When the source data changes, an event is published (via application event, database CDC, message queue) that triggers cache invalidation.
    - **Implementation patterns**:
      - Application publishes an invalidation event after writing to the database: `publish("cache:invalidate", {"entity": "product", "id": "prod_abc"})`. A cache invalidation subscriber receives the event and deletes or updates the cache entry.
      - Database CDC (Change Data Capture) via Debezium captures row-level changes and publishes events. A consumer invalidates cache entries based on the changed rows. This is more reliable than application-level events because it captures all changes, including those from migration scripts, admin tools, and direct database access.
      - Redis pub/sub for lightweight invalidation signaling between application instances (including L1 in-process cache invalidation).
    - **Advantages**: Minimizes staleness window (data is invalidated within seconds of a change, rather than waiting for TTL expiry). More precise than TTL alone.
    - **Disadvantages**: More complex infrastructure. Event delivery is not guaranteed without careful design (at-least-once delivery, idempotent invalidation). Adds a dependency on the messaging system. Does not eliminate the need for TTL (events can fail — TTL is the safety net).
    - **Always combine event-driven invalidation with TTL.** Events reduce staleness to seconds; TTL provides a guaranteed upper bound on staleness if events fail.

    **Strategy 3: Version-based invalidation** — for coordinated cache updates:
    - Instead of invalidating individual keys, increment a version counter that is part of the cache key: `v5:catalog:product:prod_abc`. When the catalog is updated, increment the version to `v6`. All new reads use `v6` keys, and old `v5` entries naturally expire via TTL.
    - **Advantages**: Atomic invalidation of all related cache entries by changing one version number. No need to enumerate and delete individual keys. Simple to implement.
    - **Disadvantages**: All cached data is abandoned on version change (even data that did not change), causing a temporary spike in cache misses. Requires a mechanism to store and distribute the current version (another cached/shared value, configuration service, or embedded in the application deployment).
    - **When to use**: For bulk updates (full catalog refresh, configuration changes, deployment-triggered data structure changes). Not suitable for fine-grained per-entity invalidation.

    **Strategy 4: Tag-based invalidation** — for group invalidation:
    - Tag cache entries with one or more labels. When a tag is invalidated, all entries with that tag are invalidated.
    - Example: Cache product `prod_abc` with tags `["category:shoes", "brand:nike"]`. When the shoes category is updated, invalidate all entries tagged `category:shoes`.
    - **Implementation**: Not natively supported by Redis or Memcached. Implement with: a reverse index (set of keys per tag in Redis: `SADD tag:category:shoes prod_abc prod_def`), or use a cache library that supports tagging (Symfony Cache, Laravel Cache tags).
    - **When to use**: When invalidation must happen at a group level (all products in a category, all data for a tenant, all cached responses for a specific upstream service).

13. **Design invalidation for common scenarios:**

    **Single entity update**: Product price changes.
    - Invalidate: `catalog:product:prod_abc` and all variants (`catalog:product:prod_abc:pricing:*`).
    - Approach: Event-driven invalidation triggered by the write operation. Use a pattern-based delete (`DEL` specific keys or `UNLINK` for non-blocking delete) if the variant set is known. Avoid `KEYS` command in production (blocks Redis) — use `SCAN` with a pattern if enumeration is needed.

    **Bulk update**: Entire product catalog is refreshed.
    - Approach: Version-based invalidation (increment catalog version). Or event-driven invalidation for each changed entity (if the changeset is bounded). Or cache warming with new data before switching the version.

    **Cascading invalidation**: A category is renamed, which affects all products in that category.
    - Approach: Identify all affected cache entries via tag-based invalidation (`tag:category:shoes`). Or accept that products will serve stale category names until TTL expiry (if the staleness is acceptable).

    **Deployment-triggered invalidation**: New code changes the cached data structure (adds/removes fields).
    - Approach: Include a schema version in the cache key prefix. Deploy new code that writes cache keys with the new version. Old-version entries expire naturally via TTL. No explicit invalidation needed.

    **User-triggered invalidation**: User updates their profile — they should see the update immediately.
    - Approach: Invalidate the user's cache entry on write. For the requesting user, bypass the cache for the next read (read-your-own-write pattern): set a short-lived flag in the user's session indicating a recent write, and skip the cache for that user for the next N seconds.
