# Phase 14: Performance Monitoring and Regression Prevention

37. **Establish the performance monitoring stack.** Define what is continuously monitored:

    **Query-level monitoring**:
    - **`pg_stat_statements`**: Enable and never disable. This is the single most important monitoring extension for PostgreSQL performance. Reset statistics periodically (weekly or monthly) to track trends. Capture snapshots to a monitoring system for historical comparison.
    - **`auto_explain`**: Automatically log execution plans for queries exceeding a threshold. Configure:
      ```
      auto_explain.log_min_duration = '500ms'  -- Log plans for queries > 500ms
      auto_explain.log_analyze = on              -- Include ANALYZE output
      auto_explain.log_buffers = on              -- Include buffer usage
      auto_explain.log_nested_statements = on    -- Include subqueries
      ```
      Use for diagnosing intermittent slow queries without manual EXPLAIN intervention.

    **Table-level monitoring**:
    - `pg_stat_user_tables`: Sequential scan counts, index scan counts, live/dead tuple counts, vacuum timestamps.
    - `pg_stat_user_indexes`: Index usage counts — detect unused indexes.
    - `pg_statio_user_tables`: Heap block reads vs. hits — detect tables with poor cache performance.

    **System-level monitoring** (via node_exporter, CloudWatch, or database-specific exporters):
    - CPU utilization (user, system, iowait).
    - Memory (total, used, cached, available, shared_buffers utilization).
    - Disk I/O (IOPS, throughput, latency, queue depth).
    - Network I/O (for replication traffic monitoring).

    **Use a metrics pipeline**: Export PostgreSQL metrics via `postgres_exporter` (Prometheus) or equivalent → Grafana dashboards. Or use managed monitoring (RDS Performance Insights, Datadog, pganalyze, Tembo insights).

38. **Design performance dashboards.** Build and maintain the following dashboards:

    **Dashboard 1: Database Health Overview**
    - Connections: active / idle / idle-in-transaction / total vs. max.
    - Transactions per second (commits + rollbacks).
    - Cache hit ratio.
    - Replication lag (all replicas).
    - Tuple operations: inserts/sec, updates/sec, deletes/sec.
    - Dead tuple count trend.
    - Disk usage and growth rate.

    **Dashboard 2: Query Performance**
    - Top 10 queries by total execution time.
    - Top 10 queries by mean execution time.
    - Top 10 queries by calls per second.
    - p50, p95, p99 latency trends for the top queries.
    - Temporary file usage (indicates spills to disk).
    - Queries currently executing > 5 seconds.

    **Dashboard 3: I/O and Resources**
    - Disk IOPS (read/write) vs. provisioned limit.
    - Disk throughput vs. provisioned limit.
    - I/O wait percentage.
    - Checkpoint frequency and duration.
    - WAL generation rate.
    - Buffer cache: blocks hit vs. blocks read (trending).

    **Dashboard 4: Lock and Contention**
    - Active locks by type and mode.
    - Lock wait count and duration.
    - Deadlock count.
    - Long-running transactions (> 1 minute).
    - Idle-in-transaction connections (> 1 minute).

39. **Design performance alerting.** Define actionable alerts:

    **Critical (page — requires immediate response)**:
    - p95 query latency for critical queries exceeds 2x SLO for > 5 minutes.
    - Connection count exceeds 85% of max_connections.
    - Replication lag exceeds 30 seconds.
    - Disk space below 10% free.
    - Database instance unreachable.
    - OOM killer invoked on the database host.
    - Deadlock rate exceeds 10/minute.

    **Warning (ticket — requires response within business hours)**:
    - Cache hit ratio drops below 99% sustained for > 15 minutes.
    - Dead tuple ratio exceeds 20% on any table with > 100k rows.
    - Longest running transaction exceeds 10 minutes.
    - Autovacuum has not completed on a large table in > 24 hours.
    - p95 query latency for critical queries exceeds 1.5x baseline for > 30 minutes.
    - Disk space below 25% free.
    - Temporary file usage exceeds 1GB/hour.
    - Unused index detected after 30+ day observation period.

    **Every alert must have a linked runbook**: symptom → diagnosis steps → mitigation steps → escalation path.

40. **Establish performance regression prevention.** Prevent performance problems before they reach production:

    **Pre-deployment checks**:
    - Every new migration must be reviewed for: locking implications, index changes, and impact on high-frequency queries.
    - Every new query pattern must be tested with `EXPLAIN (ANALYZE)` against production-scale data in staging.
    - CI pipeline should include a performance test suite that runs the critical query set and fails if latency exceeds thresholds.
    - Use `pg_stat_statements` comparison: after staging deployment, compare query statistics to the previous version. Flag any query whose mean execution time increased by > 50%.

    **Post-deployment monitoring**:
    - After every deployment, monitor the query performance dashboard for 30 minutes. Look for: new queries appearing in the top-10 by execution time, existing queries whose latency increased, and increased lock wait times.
    - Automated regression detection: compare `pg_stat_statements` snapshots pre- and post-deployment. Alert if any query's mean time increases by > 2x or if new sequential scans appear on large tables.

    **Periodic performance reviews**:
    - Monthly: Review top-20 queries by total time. Investigate any that have degraded since last month. Review index usage — identify and remove unused indexes. Review table bloat — schedule repack for bloated tables.
    - Quarterly: Capacity planning review. Update growth projections. Evaluate whether current scaling strategy will meet next quarter's demand. Review and update performance SLOs based on changing business requirements.
