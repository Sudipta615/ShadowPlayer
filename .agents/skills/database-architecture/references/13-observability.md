### Phase 13: Database Observability and Operational Readiness

41. **Design database monitoring.** Define metrics to collect and monitor for every production database:

    **Health metrics**:
    - Connection count (active, idle, waiting) vs. maximum connections.
    - Replication lag (seconds behind primary).
    - Transaction rate (commits/sec, rollbacks/sec).
    - Deadlock count.
    - Database size and growth rate.

    **Performance metrics**:
    - Query latency percentiles (p50, p95, p99) for top queries.
    - Cache hit ratio (buffer cache, index cache). PostgreSQL: `pg_stat_bgwriter`, `pg_statio_user_tables`. Target > 99% cache hit ratio — below this indicates insufficient memory.
    - Disk I/O (read/write IOPS, I/O wait time, throughput).
    - Table and index bloat (wasted space from dead tuples).
    - Vacuum and autovacuum activity (last vacuum, dead tuple count, autovacuum queue length).
    - Temporary file usage (indicates sorts or hash joins spilling to disk — need more `work_mem` or query optimization).

    **Saturation metrics**:
    - CPU utilization.
    - Memory utilization (total, used by shared buffers, used by connections).
    - Disk space (total, used, projected time to full at current growth rate).
    - WAL generation rate and WAL disk usage.

42. **Design database alerting.** Define alerts with actionable thresholds:
    - **Critical (page)**:
      - Replication lag > 30 seconds (data loss risk during failover).
      - Connection count > 80% of maximum (approaching exhaustion).
      - Disk space < 15% free (risk of database halt).
      - Database unreachable / health check failure.
      - Deadlocks > 5 per minute (indicates design problem).
      - Active long-running transactions > 5 minutes (indicates stuck query or missing timeout).
    - **Warning (ticket)**:
      - Cache hit ratio < 99%.
      - Replication lag > 5 seconds.
      - Table bloat > 30%.
      - Autovacuum not completing on large tables (dead tuple count growing).
      - Slow query count increasing trend.
      - Disk space < 30% free.
    - **Informational (dashboard)**:
      - Query patterns shifting (new high-frequency queries appearing).
      - Index usage statistics (identify unused indexes).
      - Connection pool utilization trends.

    Every critical alert must have a documented runbook: what the alert means, how to diagnose, how to mitigate, and how to escalate.

43. **Design database dashboards.** At minimum:
    - **Overview dashboard**: Connection count, replication lag, transaction rate, error rate, disk space, and CPU across all database instances.
    - **Query performance dashboard**: Top queries by total time, slowest individual queries, query count trends, and cache hit ratio.
    - **Capacity planning dashboard**: Data growth trends, disk usage projection, connection utilization trends, and index size growth.