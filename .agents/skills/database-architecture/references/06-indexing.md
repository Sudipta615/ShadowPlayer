### Phase 6: Indexing Strategy

20. **Design indexes based on the access pattern catalog.** Every index must be justified by a specific access pattern from step 2. Do not create speculative indexes — unused indexes waste storage, slow down writes, and consume memory.

    For each access pattern that involves a query:
    - **Identify the WHERE clause predicates**: These are the index candidates.
    - **Identify the ORDER BY clause**: The index should support the sort order to avoid in-memory sorts.
    - **Identify the SELECT fields**: Consider covering indexes to avoid heap lookups.

21. **Apply indexing rules (relational databases):**

    - **Single-column indexes**: For simple equality lookups on high-selectivity columns. Example: `CREATE INDEX idx_orders_customer_id ON orders(customer_id)` for "find orders by customer."
    - **Composite (multi-column) indexes**: For queries that filter on multiple columns. Column order matters — place equality predicates first, then range predicates, then sort columns. Follow the "equality, range, sort" (ERS) principle.
      - Example: For query `WHERE customer_id = ? AND status = ? ORDER BY created_at DESC`, create index `(customer_id, status, created_at DESC)`.
    - **Covering indexes**: Include frequently selected columns in the index using `INCLUDE` to avoid heap lookups entirely. Example: `CREATE INDEX idx ON orders(customer_id, status) INCLUDE (total, created_at)`.
    - **Partial indexes**: Index only a subset of rows. Dramatically reduces index size for queries that always filter by a condition. Example: `CREATE INDEX idx_active_orders ON orders(customer_id, created_at) WHERE status != 'cancelled'` — only indexes non-cancelled orders.
    - **Expression indexes**: Index computed values. Example: `CREATE INDEX idx_lower_email ON users(LOWER(email))` for case-insensitive email lookups.
    - **GIN indexes**: For JSONB fields, array containment, full-text search (`tsvector`), and trigram similarity (`pg_trgm`).
    - **BRIN indexes**: For large tables where the indexed column is naturally correlated with physical row order (e.g., `created_at` on an append-only table). Much smaller than B-tree indexes.
    - **Unique indexes**: For enforcing uniqueness constraints. Prefer unique indexes over application-level uniqueness checks — they prevent race conditions.

22. **Analyze index impact on writes.** For every index added:
    - State the write amplification cost: each insert and relevant update must update every index on the table.
    - For write-heavy tables, limit the total number of indexes. If a table has > 5-6 indexes, review whether all are necessary or whether some access patterns can be served differently (materialized views, denormalized read tables, search engine offload).
    - Monitor index usage in production. Remove indexes with zero or near-zero scans — they are pure cost with no benefit.

23. **Plan index maintenance.** Define:
    - **Index bloat monitoring and remediation**: In PostgreSQL, B-tree indexes accumulate bloat from updates and deletes. Monitor bloat ratio and reindex periodically (`REINDEX CONCURRENTLY` for zero-downtime reindexing).
    - **Statistics and query planner**: Ensure `ANALYZE` runs regularly (autovacuum handles this in PostgreSQL, but verify settings). Stale statistics cause the query planner to choose suboptimal plans.
    - **Index creation strategy for production**: Always use `CREATE INDEX CONCURRENTLY` on PostgreSQL production databases to avoid locking the table during index creation. Plan for the extra time and disk space this requires.