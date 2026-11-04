# Phase 5: Vacuum, Bloat, and Maintenance Optimization

17. **Understand and optimize autovacuum.** In PostgreSQL, MVCC creates dead tuples on every UPDATE and DELETE. VACUUM reclaims this space and updates statistics. Poor autovacuum performance is the #2 cause of PostgreSQL performance degradation (after missing indexes).

    **Diagnose autovacuum health**:
    ```sql
    SELECT
      schemaname, relname,
      n_live_tup, n_dead_tup,
      n_dead_tup::float / NULLIF(n_live_tup, 0) AS dead_ratio,
      last_vacuum, last_autovacuum,
      last_analyze, last_autoanalyze,
      autovacuum_count, autoanalyze_count
    FROM pg_stat_user_tables
    ORDER BY n_dead_tup DESC;
    ```
    - Tables with `dead_ratio > 0.1` (10% dead tuples) need more frequent vacuuming.
    - Tables where `last_autovacuum` is NULL or very old are not being vacuumed — check if autovacuum is enabled and if the thresholds are appropriate.

    **Tune autovacuum globally**:
    - `autovacuum_max_workers`: Default 3. Increase to 5-6 for databases with many large, active tables.
    - `autovacuum_naptime`: Default 1 minute. Reduce to 15-30 seconds for high-churn databases.
    - `autovacuum_vacuum_cost_delay`: Default 2ms. Reduce to 0-1ms if autovacuum is falling behind and the database has I/O headroom. This makes autovacuum more aggressive but consumes more I/O.
    - `autovacuum_vacuum_cost_limit`: Default 200. Increase to 400-1000 for faster vacuuming at the cost of more I/O.

    **Tune autovacuum per table** for high-churn tables:
    ```sql
    ALTER TABLE orders SET (
      autovacuum_vacuum_scale_factor = 0.01,  -- Vacuum when 1% of rows are dead (default 20%)
      autovacuum_analyze_scale_factor = 0.005, -- Analyze when 0.5% of rows change (default 10%)
      autovacuum_vacuum_cost_delay = 0         -- No throttling for this table
    );
    ```
    Apply to tables with millions of rows where the default 20% threshold means millions of dead tuples accumulate before vacuum triggers.

18. **Manage table bloat.** Even with proper vacuuming, PostgreSQL cannot return disk space from the middle of a table to the OS (vacuum marks space as reusable within the table, but the table file doesn't shrink):
    - **Measure table bloat**: Use `pgstattuple` or community bloat estimation queries. A table with > 30% bloat is a concern.
    - **Fix severe table bloat** (> 50%):
      - **`pg_repack`** (recommended): Rebuilds the table without holding an exclusive lock. Requires the `pg_repack` extension. Example: `pg_repack --table orders --no-superuser-check -d mydb`. This is the safest method for production.
      - **`VACUUM FULL`**: Rewrites the entire table and reclaims space, but holds an `ACCESS EXCLUSIVE` lock for the entire duration — the table is completely unavailable. Only use during maintenance windows.
      - **`CLUSTER`**: Rewrites the table ordered by a specific index. Same locking issue as `VACUUM FULL`.
    - **Prevent bloat**: Proper autovacuum tuning (step 17), avoiding long-running transactions (they prevent vacuum from reclaiming tuples visible to the old transaction snapshot), and avoiding excessive UPDATE patterns on hot rows.

19. **Address the long-running transaction problem.** Long-running transactions are one of the most dangerous performance issues in PostgreSQL:
    - **Why it matters**: PostgreSQL's MVCC requires keeping dead tuples visible to any open transaction. A single transaction open for hours prevents vacuum from cleaning up dead tuples across the entire database, causing bloat to accumulate rapidly.
    - **Detect**: 
      ```sql
      SELECT pid, now() - xact_start AS duration, state, query
      FROM pg_stat_activity
      WHERE xact_start IS NOT NULL
      ORDER BY xact_start ASC
      LIMIT 10;
      ```
    - **Set transaction timeouts**:
      - `idle_in_transaction_session_timeout`: Terminate connections that are idle within a transaction for too long (e.g., 5 minutes). This catches application bugs where a transaction is opened but never committed.
      - `statement_timeout`: Maximum execution time for any single statement. Set a global default (e.g., 30 seconds) and override per-session for known long operations.
    - **Application-level fixes**: Ensure all code paths commit or rollback transactions promptly. Use connection pool timeout settings to reclaim leaked connections. Review ORM configurations for implicit transaction management.
