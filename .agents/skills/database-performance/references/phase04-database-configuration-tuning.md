# Phase 4: Database Configuration Tuning

14. **Tune memory configuration.** Memory is the most impactful configuration category. Get it wrong, and no amount of query optimization will help. Get it right, and many queries become fast without further effort.

    **PostgreSQL memory model** (adapt for other databases):

    **`shared_buffers`** — PostgreSQL's internal buffer cache:
    - Starting point: 25% of total system RAM.
    - For dedicated database servers with > 64GB RAM: 25% is still a good starting point; going beyond 40% rarely helps because the OS file system cache handles the rest.
    - For small instances (< 4GB RAM): Set to ~512MB-1GB.
    - Measure effectiveness: Check buffer cache hit ratio:
      ```sql
      SELECT
        sum(blks_hit) * 100.0 / sum(blks_hit + blks_read) AS cache_hit_ratio
      FROM pg_stat_database
      WHERE datname = current_database();
      ```
      Target: > 99%. If below 99%, either `shared_buffers` is too small or the working set exceeds available memory.
    - After changing: Restart required. Monitor for 24-48 hours before further adjustment.

    **`effective_cache_size`** — Tells the planner how much total cache (shared_buffers + OS cache) is available:
    - Set to 50-75% of total system RAM.
    - This does not allocate memory — it only influences the planner's cost estimates. A higher value makes the planner more likely to choose index scans over sequential scans (because it assumes cached data will be available).

    **`work_mem`** — Memory per sort or hash operation:
    - Default (4MB) is conservative. For analytical or complex queries, increase to 16MB-256MB.
    - **Critical caution**: This is per-operation, not per-query or per-connection. A single complex query can use `work_mem` × N (for N sort/hash operations). A system with 100 concurrent connections running complex queries at `work_mem = 256MB` could use 100 × 3 × 256MB = 75GB.
    - Recommendation: Set the global `work_mem` conservatively (16-64MB for OLTP). For specific analytical queries or sessions that need more, set it at the session level: `SET work_mem = '256MB'`.
    - Diagnosis: Check `temp_blks_read` and `temp_blks_written` in `pg_stat_statements` — non-zero values mean queries are spilling to disk due to insufficient `work_mem`.

    **`maintenance_work_mem`** — Memory for VACUUM, CREATE INDEX, ALTER TABLE:
    - Set to 256MB-1GB (or higher for very large tables). This only applies to maintenance operations, so it can be set aggressively.
    - Higher values make VACUUM and index creation significantly faster.

    **`effective_io_concurrency`** — Number of concurrent I/O operations the OS can handle:
    - Set to 200 for SSD storage, 1-2 for spinning disk (HDD).
    - Affects bitmap heap scan and prefetching behavior.

    **`random_page_cost`** — Cost estimate for random I/O relative to sequential I/O:
    - Default: 4.0 (calibrated for HDD).
    - For SSD: Set to 1.1-1.5. This makes the planner much more likely to choose index scans, which is correct for SSD where random reads are nearly as fast as sequential reads.
    - For network-attached storage (EBS, persistent disk): Set to 1.5-2.0.
    - This is one of the most impactful settings for SSD-based deployments and is almost always misconfigured.

15. **Tune WAL and checkpoint configuration.** WAL (Write-Ahead Log) settings affect write performance, crash recovery time, and replication behavior:

    **`max_wal_size`** — Maximum WAL size before a checkpoint is forced:
    - Default: 1GB. Too low for write-heavy workloads — causes frequent checkpoints, which spike I/O.
    - Set to 4GB-16GB for write-heavy systems. Monitor checkpoint frequency in the PostgreSQL log — checkpoints triggered by `max_wal_size` (log shows "checkpoint starting: wal") should be infrequent compared to time-based checkpoints.

    **`min_wal_size`** — Minimum WAL retained:
    - Set to 1GB-2GB. Prevents aggressive WAL recycling during low-activity periods.

    **`checkpoint_completion_target`** — How much of the checkpoint interval is used to spread I/O:
    - Set to 0.9 (default in newer PostgreSQL). Spreads checkpoint writes over 90% of the interval, avoiding I/O spikes.

    **`wal_buffers`** — Shared memory for WAL writes:
    - Set to 64MB for write-heavy systems (default auto-sizing is usually fine: 1/32 of `shared_buffers`, capped at 64MB).

    **`synchronous_commit`** — Whether to wait for WAL write to disk before confirming commit:
    - `on` (default): Safest. Every commit waits for WAL to be flushed to disk. No data loss on crash.
    - `off`: Commits return before WAL is flushed. Up to ~600ms of recent commits can be lost on crash. Provides 2-5x write throughput improvement. Use only for data that can tolerate loss (session data, non-critical logs, idempotently reprocessable events).
    - Can be set per-transaction: `SET LOCAL synchronous_commit = off;` for specific non-critical writes.

16. **Tune parallelism.** PostgreSQL can parallelize queries using multiple CPU cores:

    **`max_parallel_workers_per_gather`** — Maximum parallel workers per query node:
    - Default: 2. Increase to 4-8 for analytical workloads on multi-core instances.
    - Parallel queries are most beneficial for sequential scans, hash joins, and aggregations on large tables.

    **`max_parallel_workers`** — Total parallel workers across all queries:
    - Set to the number of CPU cores (or CPU cores - 2 to leave headroom for other processes).

    **`parallel_tuple_cost` and `parallel_setup_cost`** — Planner cost estimates for parallelism:
    - Reduce these (e.g., `parallel_tuple_cost = 0.01`, `parallel_setup_cost = 100`) to make the planner more willing to use parallel plans. Adjust based on observed benefit.

    **`min_parallel_table_scan_size`** — Minimum table size for parallel scan consideration:
    - Default: 8MB. Reduce if you want parallelism on smaller tables.

    **Note**: Parallel queries consume more resources (CPU, memory per worker). On OLTP systems with many concurrent small queries, parallelism may cause resource contention. Enable aggressively for analytical/reporting queries; keep conservative for high-concurrency OLTP.
