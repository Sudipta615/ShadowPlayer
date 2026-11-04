### Phase 16: Specialized Patterns and Advanced Topics

48. **Design materialized views and precomputed data.** When read access patterns require expensive aggregations or joins:
    - **PostgreSQL materialized views**: `CREATE MATERIALIZED VIEW` with `REFRESH MATERIALIZED VIEW CONCURRENTLY` for zero-downtime refresh. Define the refresh trigger (time-based schedule, event-driven, or on-demand). Materialized views are not real-time — define the staleness tolerance.
    - **Application-managed denormalized tables**: When materialized view refresh is too slow or inflexible, maintain a separate denormalized table updated by triggers, CDC, or application events. More control, more code to maintain.
    - **Pre-aggregation tables**: For time-based analytics (daily summaries, hourly counts), compute and store aggregates on a schedule rather than computing on every query. Design the aggregation pipeline and backfill procedure.

49. **Design full-text search at the database level (if not using a dedicated search engine).** For moderate search needs within PostgreSQL:
    - Create a `tsvector` column with a GIN index.
    - Define the text search configuration (language, stopwords, synonym dictionaries).
    - Design the search ranking strategy (`ts_rank`, `ts_rank_cd`).
    - Define the threshold at which PostgreSQL full-text search should be replaced by a dedicated search engine (typically: > 10M searchable documents, complex relevance tuning needs, or faceted search requirements).

50. **Design for database testing.** Define:
    - **Local development database**: Use Docker containers running the same database engine and version as production. Never develop against SQLite when deploying to PostgreSQL — SQL dialect differences cause production bugs.
    - **Test database management**: Each test run should start with a clean, known state. Options: transactional rollback after each test (fast, but tests cannot see committed state), truncation between tests, or fresh database per test suite.
    - **Migration testing**: Run all migrations from scratch against an empty database to verify they produce the expected schema. Run new migrations against the current production schema to verify compatibility.
    - **Performance testing**: Maintain a staging database with production-scale data volume (anonymized). Run slow query detection and capacity tests against this environment.
    - **Schema drift detection**: Use tools (skeema, pgquarrel, or custom scripts) to compare the actual production schema against the expected schema defined by migrations. Alert on drift.