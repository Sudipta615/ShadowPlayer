# Phase 15: Advanced Performance Patterns

41. **Design materialized view refresh performance.** When using materialized views for read optimization:
    - **Concurrent refresh**: Always use `REFRESH MATERIALIZED VIEW CONCURRENTLY` to avoid locking the view during refresh. Requires a unique index on the materialized view.
    - **Refresh timing**: Calculate how long the refresh takes at current data volume. If it takes 30 seconds, don't refresh every 10 seconds. Define refresh frequency based on: staleness tolerance, refresh duration, and database load during refresh.
    - **Incremental refresh**: PostgreSQL does not support incremental materialized view refresh natively. If the refresh is too expensive:
      - Maintain a manually managed summary table updated by triggers or CDC events.
      - Use a dedicated analytics database (ClickHouse, TimescaleDB) for complex aggregations, fed by CDC.
    - **Monitor refresh impact**: Track CPU and I/O utilization during refresh. If refresh impacts OLTP performance, schedule during low-traffic periods or run on a dedicated replica.

42. **Design partition pruning optimization.** Ensure the query planner prunes partitions effectively:
    - The partition key column must appear in the WHERE clause with a compatible operator for pruning to work.
    - **Verify pruning**: `EXPLAIN` output should show `Subplans Removed: N` or only list the relevant partitions. If all partitions are scanned, the predicate format may not match the partition key.
    - **Common pruning failures**: Using a function on the partition key (`WHERE date_trunc('month', created_at) = '2024-01'` may not prune), type mismatches, or complex expressions. Rewrite predicates as simple range conditions: `WHERE created_at >= '2024-01-01' AND created_at < '2024-02-01'`.
    - **Runtime partition pruning** (`enable_partition_pruning = on`, default): Allows pruning based on parameter values, not just constants. Ensure it's enabled.

43. **Design connection warm-up and cache priming.** After database restarts, failovers, or new replica creation:
    - **Buffer cache is cold**: All queries hit disk, causing a spike in latency and I/O. Mitigate:
      - Use `pg_prewarm` to preload critical tables and indexes into shared buffers after restart:
        ```sql
        SELECT pg_prewarm('orders');
        SELECT pg_prewarm('idx_orders_customer_id');
        ```
      - Gradually route traffic to the restarted instance rather than sending full load immediately.
      - Enable `pg_prewarm` with `shared_preload_libraries` and `pg_prewarm.autoprewarm = on` to automatically save and restore buffer contents across restarts.
    - **Connection pool warm-up**: Pre-create minimum connections in the pool during application startup, before routing traffic.

44. **Design query plan stability.** Query plan instability (the planner choosing different plans for the same query at different times) causes intermittent performance problems:
    - **Cause**: Stale or inaccurate statistics, data distribution changes, parameter-sensitive plans (plan chosen for one parameter value is suboptimal for another).
    - **Diagnosis**: Compare `EXPLAIN` output for the same query at different times or with different parameter values. Check if the plan changes.
    - **Mitigations**:
      - Run `ANALYZE` more frequently on tables with rapidly changing data distributions.
      - Increase `default_statistics_target` for columns with non-uniform distributions (default 100, increase to 500-1000).
      - For critical queries with plan instability, consider `pg_hint_plan` (extension that lets you force specific plans) or restructure the query to guide the planner.
      - Monitor plan changes: `auto_explain` can log plans, and some monitoring tools (pganalyze) track plan changes automatically.
