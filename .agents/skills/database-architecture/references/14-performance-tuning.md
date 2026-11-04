### Phase 14: Database Performance Tuning

44. **Design database configuration tuning.** For PostgreSQL (adapt for other databases):
    - **Memory allocation**:
      - `shared_buffers`: 25% of total RAM (starting point). This is PostgreSQL's internal cache.
      - `effective_cache_size`: 50-75% of total RAM. Tells the query planner how much OS cache is available.
      - `work_mem`: Memory per sort/hash operation. Start at 4-16MB, increase for query-heavy workloads. Caution: this is per-operation, not per-connection — high values with many concurrent queries can exhaust memory.
      - `maintenance_work_mem`: Memory for maintenance operations (VACUUM, CREATE INDEX). Set to 256MB-1GB.
    - **WAL configuration**:
      - `wal_level`: `replica` for replication (or `logical` if using logical replication/CDC).
      - `max_wal_size`: Increase for write-heavy workloads to reduce checkpoint frequency.
      - `checkpoint_completion_target`: 0.9 (spread checkpoint I/O over time).
    - **Connection limits**:
      - `max_connections`: Set based on available memory (~10MB per connection) and expected concurrent connections. Use PgBouncer to allow more application connections than database connections.
    - **Autovacuum tuning**:
      - Increase `autovacuum_max_workers` for databases with many tables.
      - Decrease `autovacuum_vacuum_scale_factor` and `autovacuum_analyze_scale_factor` for large tables so autovacuum triggers more frequently.
      - Set per-table autovacuum settings for tables with unusual write patterns.
    - **Do not blindly copy configuration from the internet.** Every setting must be justified by the specific workload. Benchmark changes in staging with production-like load before applying to production.

45. **Design query optimization procedures.** For identified slow queries:
    - **Step 1**: Get the execution plan (`EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)`). Identify the most expensive node.
    - **Step 2**: Check for sequential scans on large tables — add an appropriate index or fix the WHERE clause.
    - **Step 3**: Check for nested loop joins on large datasets — ensure join columns are indexed, or investigate hash/merge join suitability.
    - **Step 4**: Check for inaccurate row estimates — run `ANALYZE` on the relevant tables, or investigate statistics target increases for columns with skewed distributions.
    - **Step 5**: Check for sorts and hash operations spilling to disk (indicated by disk-based sort/hash in the plan) — increase `work_mem` for the session or optimize the query.
    - **Step 6**: If the query cannot be optimized further, consider: restructuring the data access (materialized view, denormalized read table, CQRS), caching the result, or accepting the query is inherently expensive and moving it to a replica or async process.