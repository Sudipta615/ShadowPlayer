### Phase 10: Data Migration and Schema Evolution

34. **Design the schema migration strategy.** Define how database schemas evolve safely over time:
    - **Choose the migration tool**: Flyway (Java ecosystem, SQL-based), Alembic (Python/SQLAlchemy), golang-migrate (Go), Liquibase (multi-platform, XML/SQL), or framework-specific tools (Rails migrations, Django migrations). The tool must support: versioned migrations, checksum verification, migration history tracking, and rollback capabilities.
    - **Migration file conventions**: Sequential version numbers or timestamps, descriptive names. Example: `V20240115_01__add_tracking_number_to_orders.sql`.
    - **Migration review process**: Every migration must be reviewed by at least one other engineer with database expertise. Review for: correctness, backward compatibility, locking implications, index impact, and data integrity.
    - **Run migrations in CI**: Validate every migration against a fresh database and against a database with the previous migration state. Catch syntax errors and compatibility issues before production.

35. **Design zero-downtime migration procedures.** Schema changes must not cause application downtime. Apply the expand-and-contract pattern:

    **Adding a column**:
    1. Add the column as nullable with a default (or no default): `ALTER TABLE orders ADD COLUMN tracking_number VARCHAR(100)`. This is non-blocking in PostgreSQL (if no volatile default).
    2. Deploy application code that writes to the new column.
    3. Backfill existing rows if needed.
    4. Add NOT NULL constraint if required (after backfill): `ALTER TABLE orders ALTER COLUMN tracking_number SET NOT NULL`.

    **Renaming a column**:
    1. Add the new column.
    2. Deploy code that writes to both old and new columns.
    3. Backfill the new column from the old column.
    4. Deploy code that reads from the new column.
    5. Deploy code that stops writing to the old column.
    6. Drop the old column.

    **Changing a column type**:
    1. Add a new column with the target type.
    2. Dual-write to both columns.
    3. Backfill.
    4. Switch reads to the new column.
    5. Stop writing to the old column.
    6. Drop the old column.

    **Dropping a column or table**:
    1. Deploy code that stops reading from the column/table.
    2. Deploy code that stops writing to the column/table.
    3. Wait for all application versions that reference the column to be fully drained.
    4. Drop the column/table.

    **General rules**:
    - Never rename or drop a column in the same deployment that changes the code reading it.
    - Never add a NOT NULL column without a default in a single step — this locks the table while rewriting all rows.
    - Test every migration against a production-sized dataset in staging to measure execution time and locking behavior.

36. **Design data migration between database technologies.** When migrating from one database to another:
    - **Define the migration strategy**:
      - **Dual-write with gradual cutover**: Write to both old and new databases. Read from old. Gradually shift reads to new. Compare results for correctness. Cut over writes when confident. Highest safety, highest complexity.
      - **CDC-based replication**: Use CDC to continuously replicate from old to new. Verify data consistency. Cut over reads, then writes. Most practical for large datasets.
      - **Big bang migration**: Stop the old system, migrate data, start the new system. Simplest but requires downtime. Acceptable only for small datasets or when downtime is tolerable.
    - **Define the verification strategy**: Row counts, checksum comparison, sample query comparison between old and new systems.
    - **Define the rollback plan**: How to revert to the old system if the new system has issues. This must be defined and tested before migration begins.
    - **Define the timeline**: Schema creation → data migration → verification → shadow traffic → gradual cutover → old system decommission.