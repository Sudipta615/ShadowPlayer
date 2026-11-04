# Phase 3: Index Performance Engineering

9. **Diagnose missing indexes.** Identify queries that would benefit from indexes using multiple signals:

   **Signal 1: Sequential scans on large tables** in execution plans (from step 5b).

   **Signal 2: `pg_stat_user_tables` scan statistics**:
   ```sql
   SELECT
     schemaname, relname,
     seq_scan, seq_tup_read,
     idx_scan, idx_tup_fetch,
     n_live_tup
   FROM pg_stat_user_tables
   WHERE n_live_tup > 10000
   ORDER BY seq_scan DESC;
   ```
   Tables with high `seq_scan` count and many live tuples are candidates for missing indexes.

   **Signal 3: `pg_stat_statements` queries with high `shared_blks_read`** relative to rows returned — reading many blocks to return few rows indicates lack of index.

   **Signal 4: Application-level monitoring** showing specific queries with high latency.

   For each identified candidate, design the index following the principles in step 10.

10. **Design indexes with precision.** Every index must be designed for a specific query or set of queries. Follow this systematic process:

    **Step 10a: Identify the query's predicate columns** (WHERE clause). These are the primary index candidates.

    **Step 10b: Determine the column order using the ERS rule**:
    - **Equality** predicates first (e.g., `customer_id = ?`, `status = ?`).
    - **Range** predicates next (e.g., `created_at > ?`, `amount BETWEEN ? AND ?`).
    - **Sort** columns last (e.g., `ORDER BY created_at DESC`).

    Example: Query `WHERE customer_id = ? AND status IN ('active', 'pending') AND created_at > ? ORDER BY created_at DESC`
    → Index: `(customer_id, status, created_at DESC)`

    **Step 10c: Consider a covering index** if the query selects only a few columns. Add them with `INCLUDE`:
    ```sql
    CREATE INDEX idx_orders_customer_status_date
    ON orders (customer_id, status, created_at DESC)
    INCLUDE (total_amount, order_number);
    ```
    This allows an index-only scan — the database never touches the heap table, which is dramatically faster.

    **Step 10d: Consider a partial index** if the query always has a constant filter:
    ```sql
    -- If the application only ever queries non-deleted orders:
    CREATE INDEX idx_active_orders_customer
    ON orders (customer_id, created_at DESC)
    WHERE deleted_at IS NULL;
    ```
    This index is smaller, faster to scan, and faster to maintain than a full index.

    **Step 10e: Consider the index's selectivity.** An index on a column with only 3 distinct values (e.g., `status` with values 'active', 'cancelled', 'completed') is nearly useless alone — the database will prefer a sequential scan because the index matches too many rows. However, `status` combined with other columns in a composite index is useful when the combination is selective.

    **Step 10f: Verify the new index is used.** After creating the index, run the target query with `EXPLAIN (ANALYZE)` and confirm the planner uses the new index. If it doesn't:
    - Check if statistics are stale (`ANALYZE` the table).
    - Check if `random_page_cost` is too high for SSD storage (default 4.0, set to 1.1-1.5 for SSD).
    - Check if `enable_indexscan` or `enable_indexonlyscan` is disabled (should be `on`).
    - As a diagnostic, try `SET enable_seqscan = off` and re-explain — if the index plan is chosen, compare the costs to understand why the planner prefers seq scan.

11. **Diagnose and remove unused indexes.** Unused indexes are pure cost — they consume storage, slow down writes, and waste memory. Identify them:
    ```sql
    SELECT
      schemaname, tablename, indexname,
      idx_scan AS times_used,
      pg_size_pretty(pg_relation_size(indexrelid)) AS index_size
    FROM pg_stat_user_indexes
    WHERE idx_scan = 0
      AND indexrelid NOT IN (
        SELECT conindid FROM pg_constraint WHERE contype IN ('p', 'u')
      )
    ORDER BY pg_relation_size(indexrelid) DESC;
    ```
    - **Review period**: Only drop indexes that have had zero scans for at least 30 days (or one full business cycle including month-end, quarter-end patterns).
    - **Exclude**: Primary key indexes, unique constraint indexes (they enforce integrity, not just query performance), and indexes that might be used by background jobs that run infrequently.
    - **Drop safely**: `DROP INDEX CONCURRENTLY` to avoid locking the table. Keep the CREATE INDEX statement in a rollback script.

12. **Diagnose and resolve duplicate and overlapping indexes.** Find indexes that are redundant:
    ```sql
    -- Find indexes where one is a left-prefix of another
    SELECT
      a.indexrelid::regclass AS shorter_index,
      b.indexrelid::regclass AS longer_index,
      pg_size_pretty(pg_relation_size(a.indexrelid)) AS shorter_size
    FROM pg_index a
    JOIN pg_index b ON a.indrelid = b.indrelid
      AND a.indexrelid != b.indexrelid
      AND a.indkey::text = ANY(
        ARRAY[
          (SELECT string_agg(x::text, ' ') FROM unnest(b.indkey[1:array_length(a.indkey, 1)]) x)
        ]
      )
    WHERE a.indisunique = false;
    ```
    - An index on `(customer_id)` is redundant if an index on `(customer_id, created_at)` exists — the composite index serves both single-column and two-column lookups.
    - Exception: if the single-column index is significantly smaller and the single-column query is extremely frequent, keeping both may be justified. Measure before deciding.

13. **Manage index bloat.** In PostgreSQL, B-tree indexes accumulate dead entries from updates and deletes. Bloated indexes are larger than necessary, consume more I/O, and degrade scan performance:
    - **Measure bloat**: Use the `pgstattuple` extension:
      ```sql
      SELECT * FROM pgstatindex('idx_orders_customer_id');
      ```
      Look at `avg_leaf_density` — below 70% indicates significant bloat.
    - **Alternatively**, estimate bloat using the community bloat estimation queries (from pgexperts or check_postgres).
    - **Fix bloat**: `REINDEX INDEX CONCURRENTLY idx_orders_customer_id` — rebuilds the index without locking the table. Requires PostgreSQL 12+ for CONCURRENTLY. Plan for temporary extra disk space (two copies of the index exist during reindex).
    - **Prevent excessive bloat**: Ensure autovacuum runs frequently enough to clean up dead tuples. For tables with very high update rates, tune per-table autovacuum settings (see Phase 5).
