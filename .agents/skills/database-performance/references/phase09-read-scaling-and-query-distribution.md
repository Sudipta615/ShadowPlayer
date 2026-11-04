# Phase 9: Read Scaling and Query Distribution

28. **Design read replica strategy for performance.** When the primary database cannot handle the read load:

    **Step 28a: Identify which read queries can tolerate replication lag.**
    - Queries where data freshness of 1-5 seconds is acceptable: customer-facing list views, dashboards, search results, product catalogs, reports.
    - Queries that MUST read from primary: reads immediately following a write by the same user (read-your-own-write consistency), real-time inventory checks before purchase, account balance checks.

    **Step 28b: Configure read replica routing.**
    - **Application-level routing**: The application chooses primary or replica based on the query type. Use a connection wrapper or middleware that routes read-only queries to replicas and write queries to the primary. Example pattern:
      ```
      @read_replica
      def get_orders(customer_id):
          ...
      
      @primary
      def create_order(order_data):
          ...
      ```
    - **Proxy-level routing**: Use a proxy (PgBouncer with read/write splitting, ProxySQL, RDS Proxy, or HAProxy) that routes based on query type (SELECT → replica, INSERT/UPDATE/DELETE → primary). Less code change, but less control over edge cases.

    **Step 28c: Handle read-your-own-write consistency.**
    - After a write, route that user's subsequent reads to the primary for a configured duration (e.g., 5 seconds — longer than the maximum replication lag).
    - Implementation: Set a flag in the user's session/cookie after a write, and use it to route reads to the primary until the flag expires.
    - Alternative: Include a `last_write_timestamp` in the session and compare it to the replica's replay position — route to the replica only if the replica has caught up past that timestamp.

    **Step 28d: Monitor replication lag.** Measure and alert on replication lag:
    ```sql
    -- On the replica:
    SELECT
      now() - pg_last_xact_replay_timestamp() AS replication_lag;
    ```
    Alert if lag exceeds 5 seconds (warning) or 30 seconds (critical). Investigate: is the replica under-resourced? Is the primary generating more WAL than the replica can replay? Is a long-running query on the replica delaying replay?

    **Step 28e: Handle replica failure.** The application must handle replica unavailability:
    - If a replica goes down, route reads to the primary (with degraded capacity — alert the team).
    - Use health checks to detect replica availability and lag. Remove laggy replicas from the read pool until they catch up.

29. **Design query result caching.** Complement read replicas with caching to further reduce database load:
    - **Identify cache candidates**: Queries with high frequency, stable results, and tolerance for staleness. Examples: product details (cache 5 minutes), user profile (cache 1 minute), category list (cache 1 hour), dashboard aggregates (cache 30 seconds).
    - **Design cache keys**: Use a deterministic key that includes all query parameters: `orders:customer:{customer_id}:status:{status}:page:{cursor}`. Include a version prefix for cache invalidation on schema changes: `v2:orders:...`.
    - **Design invalidation**:
      - **TTL-based** (simplest): Cache expires after a fixed duration. Appropriate when staleness is acceptable. Define TTL per entity based on update frequency and freshness requirements.
      - **Event-based invalidation**: When data is written, publish an event that invalidates or updates the cache. More complex but ensures freshness. Use for data where staleness causes user-visible problems (inventory counts, order status).
      - **Cache-aside pattern**: Application checks cache → miss → query database → populate cache. This is the default pattern. Pair with TTL for expiration.
    - **Cache stampede prevention**: When a popular cache key expires, hundreds of concurrent requests may hit the database simultaneously. Mitigate with:
      - **Lock-based repopulation**: First request acquires a lock and repopulates; others wait or serve stale data.
      - **Probabilistic early expiration**: Each request has a small probability of refreshing the cache before TTL expires, spreading the refresh load.
      - **Background refresh**: A background process refreshes cache entries before they expire, so the cache is never empty.
