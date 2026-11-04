# Phase 2: Query-Level Performance Analysis

4. **Identify the problematic queries.** Use the database's query statistics infrastructure to find queries that need attention. Prioritize using the following order — this is critical because optimizing the wrong query wastes effort:

   **Priority 1: Queries with the highest total execution time.** `total_exec_time` in `pg_stat_statements`. These are the queries consuming the most database resources in aggregate. A query that runs in 50ms but is called 100,000 times per hour (total: 5,000 seconds/hour) has far more impact than a query that runs in 5 seconds but is called once per hour (total: 5 seconds/hour). Always start here.

   **Priority 2: Queries with the highest mean execution time that are called frequently.** These are the queries where per-execution optimization yields the most benefit per call.

   **Priority 3: Queries with high variance** (mean execution time much lower than max execution time). These indicate intermittent performance problems — lock contention, cache misses on cold data, or plan instability.

   **Priority 4: Queries that recently degraded.** Compare current statistics to historical baselines. Queries whose mean execution time increased significantly indicate a regression (data growth, plan change, missing index, bloat).

   For PostgreSQL, use:
   ```sql
   SELECT
     queryid,
     calls,
     total_exec_time / 1000 AS total_seconds,
     mean_exec_time AS mean_ms,
     max_exec_time AS max_ms,
     stddev_exec_time AS stddev_ms,
     rows,
     shared_blks_hit,
     shared_blks_read,
     (shared_blks_hit::float / NULLIF(shared_blks_hit + shared_blks_read, 0) * 100)::numeric(5,2) AS cache_hit_pct,
     query
   FROM pg_stat_statements
   ORDER BY total_exec_time DESC
   LIMIT 20;
   ```

5. **Analyze the execution plan for each problematic query.** Use `EXPLAIN (ANALYZE, BUFFERS, COSTS, FORMAT TEXT)` — never just `EXPLAIN` without `ANALYZE`, because the estimated plan can differ dramatically from the actual plan. For production queries that cannot be rerun safely, use `EXPLAIN (BUFFERS, COSTS, FORMAT TEXT)` without `ANALYZE`, or capture plans via `auto_explain`.

   Read the execution plan systematically:

   **Step 5a: Identify the most expensive node.** Look at the `actual time` and `rows` for each node. The node with the highest actual time or the largest gap between estimated and actual rows is the primary target.

   **Step 5b: Check for sequential scans on large tables.** A sequential scan (`Seq Scan`) is acceptable on small tables (< 10,000 rows) or when the query genuinely needs most of the table's rows (> 5-10% selectivity). A sequential scan on a large table when only a few rows are needed indicates a missing index or an un-indexable predicate.
   - Diagnosis: Check the WHERE clause. Is there an index on the filtered columns? If yes, is the index being ignored? (Possible reasons: type mismatch in the predicate, function applied to indexed column, OR conditions, low selectivity making seq scan cheaper, stale statistics.)

   **Step 5c: Check for row estimate accuracy.** Compare `rows=N` (estimated) vs. `actual rows=M` for each node. If the estimate is off by more than 10x, the query planner is making suboptimal decisions based on bad statistics.
   - Fix: Run `ANALYZE` on the table. If still inaccurate, increase `default_statistics_target` for the specific column (e.g., `ALTER TABLE orders ALTER COLUMN status SET STATISTICS 1000`). For correlated columns, consider extended statistics (`CREATE STATISTICS`).

   **Step 5d: Check for nested loop joins on large datasets.** Nested loop joins are efficient when the inner side is small or indexed. If both sides are large (> 10,000 rows), a hash join or merge join is usually better. A nested loop on large datasets usually indicates a missing index on the join column or an incorrect row estimate that made the planner choose nested loop incorrectly.

   **Step 5e: Check for disk-based operations.** Look for `Sort Method: external merge Disk` or `Batches: N` in hash operations. These indicate the operation exceeded `work_mem` and spilled to disk.
   - Fix: Increase `work_mem` for the session (`SET work_mem = '64MB'`) and re-test. If the query is common, consider increasing `work_mem` globally (but calculate the memory impact: `work_mem × max_connections × operations_per_query`).

   **Step 5f: Check buffer usage.** The `Buffers:` section shows `shared hit` (data found in cache) vs. `shared read` (data read from disk). A high ratio of reads to hits indicates cold cache or working set exceeding available memory.

   **Step 5g: Check for unnecessary columns.** If the query selects `SELECT *` but only a few columns are needed, excess columns increase I/O and memory usage. This is especially impactful for tables with large text/JSONB columns.

6. **Apply query-level optimizations.** Based on the execution plan analysis, apply targeted fixes. For each fix, re-run `EXPLAIN (ANALYZE, BUFFERS)` and compare before/after metrics:

   **Missing index**: Create the appropriate index (see Phase 3 for detailed index design). Verify the planner uses the new index.

   **Suboptimal join order**: The planner usually gets this right if statistics are accurate. If not, ensure statistics are fresh (`ANALYZE`). As a last resort, use `SET join_collapse_limit` or CTE materialization to control join order — but document why and recheck after PostgreSQL upgrades.

   **Inefficient subqueries**: Replace correlated subqueries (which execute once per outer row) with JOINs or lateral joins. Replace `IN (SELECT ...)` with `EXISTS (SELECT 1 ...)` when the subquery returns many rows — `EXISTS` short-circuits.

   **OR conditions preventing index usage**: `WHERE status = 'active' OR status = 'pending'` can sometimes prevent index usage. Rewrite as `WHERE status IN ('active', 'pending')` — same semantics, but more optimizer-friendly. For complex OR conditions across different columns, consider `UNION ALL` of separate indexed queries.

   **Function calls on indexed columns**: `WHERE LOWER(email) = 'user@example.com'` cannot use a standard index on `email`. Create an expression index: `CREATE INDEX idx_lower_email ON users (LOWER(email))`. Or: `WHERE date_trunc('day', created_at) = '2024-01-15'` — create an expression index or rewrite as a range: `WHERE created_at >= '2024-01-15' AND created_at < '2024-01-16'`.

   **Type mismatches**: `WHERE id = '123'` when `id` is an integer forces a cast and may prevent index usage. Ensure application code sends the correct type.

   **LIKE with leading wildcard**: `WHERE name LIKE '%smith%'` cannot use a B-tree index. Use `pg_trgm` GIN index for substring matching: `CREATE INDEX idx_trgm_name ON users USING GIN (name gin_trgm_ops)`. Or offload to a full-text search engine for complex text queries.

   **Excessive JOINs**: If a query joins 8+ tables, consider whether all joins are necessary for the result set. Can some data be denormalized or pre-joined in a materialized view? Can the query be decomposed into multiple simpler queries executed by the application?

   **COUNT(*) on large tables**: `SELECT COUNT(*) FROM large_table` always requires a full scan in PostgreSQL (MVCC means there is no single "row count"). Mitigation options: maintain an approximate count in a counter table updated by triggers/events, use `SELECT reltuples::bigint FROM pg_class WHERE relname = 'large_table'` for an estimate, or cache the count with a TTL. Choose based on accuracy requirements.

7. **Optimize pagination queries.** Pagination is one of the most common sources of database performance problems at scale:

   **Offset pagination degradation**: `SELECT * FROM orders ORDER BY created_at DESC LIMIT 20 OFFSET 100000` — the database must scan and discard 100,000 rows before returning 20. Performance degrades linearly with offset depth.
   - **Fix: Switch to keyset (cursor-based) pagination**:
     ```sql
     SELECT * FROM orders
     WHERE (created_at, id) < ('2024-01-10T12:00:00Z', 'ord_5000')
     ORDER BY created_at DESC, id DESC
     LIMIT 20
     ```
     This uses the index directly and performs consistently regardless of page depth. Requires a compound index on `(created_at DESC, id DESC)`.

   **COUNT for total results**: Avoid `SELECT COUNT(*) FROM orders WHERE ...` before every paginated query — it doubles the work. Options: return `has_more: true/false` instead of total count (check by fetching `LIMIT + 1` rows), cache the count with a TTL, or compute it only on the first page request.

8. **Optimize bulk and batch operations.** For write-heavy workloads:

   **Bulk inserts**:
   - Use multi-value INSERT: `INSERT INTO orders (col1, col2) VALUES (v1, v2), (v3, v4), ...` — batch 100-1000 rows per statement. Single-row inserts with per-row round-trips are orders of magnitude slower.
   - Use `COPY` (PostgreSQL) for very large data loads — it bypasses SQL parsing and is the fastest ingestion method.
   - Disable indexes and constraints during large loads, then rebuild. Only for initial data loads, not for online operation.
   - Wrap batches in explicit transactions to avoid per-row transaction overhead.

   **Bulk updates**:
   - Use `UPDATE ... FROM` with a values list or temporary table for batch updates on known rows:
     ```sql
     UPDATE orders SET status = tmp.status
     FROM (VALUES ('ord_1', 'shipped'), ('ord_2', 'delivered')) AS tmp(id, status)
     WHERE orders.id = tmp.id;
     ```
   - For large-scale updates (millions of rows), process in batches of 1,000-10,000 with a brief pause between batches to avoid lock contention, WAL pressure, and replication lag.

   **Bulk deletes**:
   - Never `DELETE FROM large_table WHERE condition` on millions of rows in a single transaction. This holds locks, generates massive WAL, and causes replication lag.
   - Delete in batches:
     ```sql
     DELETE FROM orders WHERE id IN (
       SELECT id FROM orders WHERE status = 'expired' AND created_at < '2023-01-01'
       LIMIT 5000
     );
     ```
     Repeat in a loop with a 100-500ms pause between batches. Monitor replication lag during the process.
   - For partitioned tables, use `DROP` or `DETACH PARTITION` instead of row-by-row deletion — this is instantaneous.
