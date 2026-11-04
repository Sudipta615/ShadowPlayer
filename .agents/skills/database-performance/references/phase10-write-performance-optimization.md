# Phase 10: Write Performance Optimization

30. **Optimize write throughput.** When write performance is the bottleneck:

    **Batch writes**: Group multiple INSERT/UPDATE statements into single transactions with multi-value inserts. Commit every 100-1000 rows. Per-row commits have 10-100x overhead due to fsync per commit.

    **Asynchronous commit for non-critical writes**: Set `synchronous_commit = off` for writes where the data can be reconstructed (event logs, analytics events, cache warming). This eliminates the fsync wait on each commit, providing 2-5x throughput improvement.

    **Unlogged tables for ephemeral data**: `CREATE UNLOGGED TABLE` for session data, temporary processing tables, or cache tables. Unlogged tables do not write WAL, making writes 2-3x faster. Data is lost on crash — use only for data that can be reconstructed.

    **Reduce index overhead on write-heavy tables**: Each index on a table slows down every INSERT and UPDATE. For write-heavy tables, audit indexes rigorously — remove any that are not serving critical read paths. Consider creating indexes asynchronously (write to an unindexed staging table, then batch-merge into the indexed main table).

    **Avoid unnecessary UPDATE triggers and constraints**: Each trigger fires per-row and adds latency. Defer non-critical post-write processing to async events.

    **HOT updates (Heap-Only Tuple) optimization in PostgreSQL**: When an UPDATE modifies only non-indexed columns and the new tuple fits on the same page, PostgreSQL can perform a HOT update — no index updates needed, significantly faster. To maximize HOT updates:
    - Keep `fillfactor` below 100 (e.g., 70-80) on frequently updated tables: `ALTER TABLE orders SET (fillfactor = 80)`. This leaves free space on each page for HOT updates.
    - Avoid indexing columns that are frequently updated (e.g., don't index `status` or `updated_at` unless a critical query pattern requires it).
    - Monitor HOT update ratio:
      ```sql
      SELECT relname, n_tup_upd, n_tup_hot_upd,
        (n_tup_hot_upd::float / NULLIF(n_tup_upd, 0) * 100)::numeric(5,2) AS hot_pct
      FROM pg_stat_user_tables
      WHERE n_tup_upd > 0
      ORDER BY n_tup_upd DESC;
      ```
      Target: > 90% HOT updates for frequently updated tables.

31. **Optimize write-heavy schema design.** When the schema itself limits write performance:

    **Partitioned writes**: For append-heavy workloads (logs, events, time-series), partition by time. New data always goes to the latest partition, avoiding contention with queries on historical data. Old partitions can be detached and archived without affecting write performance.

    **Queue table anti-pattern**: Using a database table as a job queue (`SELECT ... FOR UPDATE SKIP LOCKED`) works at low volume but degrades at high volume due to index contention, bloat from rapid insert/delete cycles, and vacuum pressure. At > 1,000 jobs/second, migrate to a dedicated message queue (SQS, RabbitMQ, Redis streams).

    **Sequence contention**: `SERIAL`/`BIGSERIAL` columns use a PostgreSQL sequence, which is a point of contention at very high insert rates (> 50,000 inserts/sec). Mitigate with: `CACHE` on the sequence (`ALTER SEQUENCE orders_id_seq CACHE 100`), or switch to UUIDv7 (generated in the application, no database round-trip).
