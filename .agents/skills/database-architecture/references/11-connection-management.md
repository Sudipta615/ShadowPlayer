### Phase 11: Connection Management and Resource Optimization

37. **Design connection pooling.** Database connections are expensive resources — each connection consumes memory on the database server (PostgreSQL: ~10MB per connection). Mismanaged connections are the #1 cause of database-related production incidents.
    - **Application-level connection pool**: Every application instance must use a connection pool (HikariCP for JVM, SQLAlchemy pool for Python, pgx pool for Go). Define:
      - Minimum pool size: Keep a small number of idle connections ready (e.g., 2-5).
      - Maximum pool size: Calculate based on `max_connections / number_of_instances` with headroom. If the database allows 200 connections and you have 10 application instances, each instance gets at most 15-18 connections (reserve some for admin, monitoring, migrations).
      - Connection timeout: How long to wait for a connection from the pool before failing (e.g., 5 seconds). Never wait indefinitely.
      - Idle timeout: Return idle connections after a period (e.g., 10 minutes) to free database resources.
      - Connection validation: Test connections before use (`SELECT 1` or equivalent) to handle stale connections after network issues.
    - **External connection pooler** (recommended for high-instance-count deployments): Use PgBouncer (PostgreSQL) or ProxySQL (MySQL) as an intermediary:
      - **Transaction pooling mode** (recommended): Connections are returned to the pool after each transaction. Allows many application connections to share few database connections. Caveat: session-level features (prepared statements, temp tables, SET commands) do not work across transaction boundaries. Ensure the application is compatible.
      - **Session pooling mode**: Connections are dedicated for the duration of the application's session. Less efficient but compatible with session-level features.
    - **Serverless/managed connection proxies**: RDS Proxy, Cloud SQL Proxy — managed connection pooling that handles IAM authentication and failover. Recommended when using managed databases with auto-scaling application instances.

38. **Design query performance management.** Establish practices for ongoing query health:
    - **Slow query logging**: Enable logging of queries exceeding a threshold (e.g., 100ms in production, 50ms in staging). Include query text, execution time, and plan.
    - **Query plan analysis**: For critical queries, capture and review `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` output. Look for: sequential scans on large tables, nested loop joins on large datasets, high buffer read counts, and inaccurate row estimates.
    - **pg_stat_statements** (PostgreSQL) or equivalent: Enable query statistics collection to identify the most time-consuming queries in aggregate (total time, call count, mean time). Focus optimization on queries that consume the most total time, not just the slowest individual queries.
    - **Connection and lock monitoring**: Monitor active connections, waiting connections, and lock waits. Alert on connection count approaching `max_connections` and on lock waits exceeding a threshold (e.g., 5 seconds).