# Phase 6: Connection Performance Management

20. **Diagnose connection problems.** Connection issues manifest as: "too many connections" errors, connection timeout from the application, high latency due to connection pool wait time, or database memory exhaustion from too many connections.

    **Diagnose current state**:
    ```sql
    SELECT
      state,
      count(*) AS count,
      max(now() - state_change) AS max_duration
    FROM pg_stat_activity
    WHERE backend_type = 'client backend'
    GROUP BY state
    ORDER BY count DESC;
    ```
    Common findings:
    - Many `idle` connections: Application is holding connections open without using them. Fix: tune application pool idle timeout.
    - Many `idle in transaction` connections: Application opens transactions and doesn't commit/rollback promptly. Fix: set `idle_in_transaction_session_timeout`, fix application code.
    - Many `active` connections: Genuine high concurrency, or slow queries holding connections. Fix: optimize slow queries, add read replicas, or implement queuing.
    - Connections near `max_connections`: Connection exhaustion risk. Fix: implement connection pooling (PgBouncer), reduce per-instance pool sizes, or increase `max_connections` (but this increases memory usage).

21. **Design and tune connection pooling.** Connection pooling is not optional for production systems — it is mandatory:

    **Application-level pool sizing formula**:
    ```
    optimal_pool_size = ((core_count * 2) + effective_spindle_count)
    ```
    For SSD: `effective_spindle_count = 1`. So for a 4-core instance: `(4 * 2) + 1 = 9`. This is per application instance.

    A common mistake is setting the pool too large. Excessive connections cause context switching overhead on the database. A smaller pool with queued requests often outperforms a larger pool with more active connections.

    **Configure the pool**:
    - `maximumPoolSize`: Calculate as `max_connections / number_of_app_instances` with 20% headroom for admin, monitoring, and migration connections. If `max_connections = 200` and you have 10 instances, each instance gets at most 16 connections.
    - `minimumIdle`: 2-5 connections. Keeps a warm pool for immediate use.
    - `connectionTimeout`: 5-10 seconds. How long the application waits for a pool connection before throwing an error. Set this to fail fast — waiting 30 seconds for a connection while the user waits is worse than returning an error.
    - `idleTimeout`: 10 minutes. Return idle connections to free database resources.
    - `maxLifetime`: 30 minutes. Rotate connections to handle database failovers and DNS changes gracefully. Set slightly less than any database-side timeout.
    - `leakDetectionThreshold`: Enable (e.g., 30 seconds). Logs a warning if a connection is checked out and not returned within the threshold — catches connection leak bugs.

    **External connection pooler (PgBouncer)** — deploy when:
    - The total connections from all application instances exceed `max_connections`.
    - Auto-scaling application instances make per-instance pool sizing unpredictable.
    - Serverless functions (Lambda) create ephemeral connections.
    
    PgBouncer configuration:
    - `pool_mode = transaction` (recommended): Connections are shared between clients at transaction boundaries. Supports the highest client-to-server connection ratio. Caveat: session-level state (prepared statements, temp tables, SET variables) is not preserved between transactions.
    - `default_pool_size`: Number of server connections per database-user pair. Start with 20-50 and adjust based on observed concurrency.
    - `max_client_conn`: Maximum client connections PgBouncer accepts. Set high (1000+) — PgBouncer handles idle client connections efficiently.
    - `reserve_pool_size`: Extra connections available during bursts. Set to 5-10.
    - `server_idle_timeout`: Close idle server connections after this period. Set to 600 seconds.
    - `query_wait_timeout`: How long a client waits for a server connection. Set to 10-30 seconds.
