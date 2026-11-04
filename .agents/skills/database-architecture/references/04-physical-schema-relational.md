### Phase 4: Physical Schema Design (Relational Databases)

11. **Determine the normalization level and justify it.** Start with Third Normal Form (3NF) as the default and deviate intentionally:
    - **3NF (default)**: Eliminates data redundancy, ensures data integrity, and simplifies updates. Use for the system of record and any data that is frequently updated.
    - **Controlled denormalization**: Introduce only when a specific, measured access pattern requires it and the performance benefit is demonstrated. For each denormalization:
      - State which access pattern it serves (by number from the catalog).
      - State what data is duplicated and where.
      - Define the update propagation strategy — how is the denormalized copy kept in sync? (Trigger, application code, event-driven update, materialized view.)
      - State the risk: data inconsistency if the sync mechanism fails.
    - **Common justified denormalizations**:
      - Storing a computed count (e.g., `order_count` on Customer) to avoid COUNT queries on large tables. Update via trigger or application code.
      - Storing a snapshot of related data at a point in time (e.g., product name and price on OrderLineItem at the time of purchase — this is not denormalization, it is intentional snapshotting because the source data may change).
      - Creating a read-optimized table or materialized view for a specific reporting query.

12. **Design the table structure.** For each table:
    - **Table name**: Plural, snake_case (e.g., `orders`, `order_line_items`, `customer_addresses`).
    - **Primary key**: Define the PK strategy:
      - **UUID (uuid/uuid_v7)**: Recommended for distributed systems, avoids sequential ID enumeration, safe for external exposure. UUIDv7 is preferred over UUIDv4 because it is time-ordered, which provides better index locality and insert performance in B-tree indexes.
      - **BIGSERIAL (auto-increment)**: Simpler, smaller storage, better index performance than random UUIDs. Use when the system is single-database, IDs are not externally exposed, and the simplicity benefit outweighs the distribution limitation.
      - **Natural key**: Use only when a stable, immutable, globally unique natural identifier exists (e.g., ISO country code). Rarely appropriate for primary entities.
      - State the choice and justification per table.
    - **Columns**: Define each column with:
      - Name (snake_case).
      - Data type (be specific: `VARCHAR(255)` not just "string"; `NUMERIC(12,2)` for money, not `FLOAT`; `TIMESTAMPTZ` not `TIMESTAMP`).
      - Nullability (NOT NULL by default — make columns nullable only when null has a defined semantic meaning).
      - Default value if applicable.
      - Constraints (UNIQUE, CHECK, FOREIGN KEY).
    - **Audit columns**: Include `created_at TIMESTAMPTZ NOT NULL DEFAULT now()` and `updated_at TIMESTAMPTZ NOT NULL DEFAULT now()` on every table. Use a trigger or application code to maintain `updated_at`.
    - **Soft delete**: If the system requires soft delete, add `deleted_at TIMESTAMPTZ NULL`. Define whether queries automatically filter deleted records (application-level concern or database view). State the tradeoff: soft delete complicates queries (every query must filter `WHERE deleted_at IS NULL`) but enables recovery and audit trails. If the primary motivation is audit trail, consider an audit log table instead of soft delete.

13. **Design foreign key relationships.** For each relationship:
    - Define the FK column (e.g., `customer_id BIGINT NOT NULL REFERENCES customers(id)`).
    - Define the ON DELETE behavior:
      - `CASCADE`: Child records are deleted when parent is deleted. Use for strong ownership (Order → LineItems).
      - `RESTRICT` / `NO ACTION`: Prevent parent deletion if children exist. Use when children should outlive the parent relationship or deletion should be explicit.
      - `SET NULL`: Set FK to NULL when parent is deleted. Use when the relationship is optional and historical reference should be preserved without the parent.
    - Define the ON UPDATE behavior (usually `CASCADE` or `NO ACTION`).
    - **Index every foreign key column.** Unindexed FKs cause full table scans on parent deletes and join operations. This is one of the most common performance mistakes.

14. **Design enum and status fields.** Choose one approach consistently:
    - **PostgreSQL ENUM type**: Type-safe, compact storage. Drawback: adding values requires `ALTER TYPE`, which acquires a lock. Acceptable for small, rarely changing enums.
    - **VARCHAR with CHECK constraint**: More flexible — adding values requires altering the CHECK constraint, which is simpler. Recommended for enums that may grow.
    - **Lookup/reference table**: For enums that have additional attributes (e.g., status table with status code, display name, description, sort order). Use FK from the main table to the lookup table.
    - **Integer codes**: Avoid. They are unreadable in query results and debugging.
    - State the convention and apply it across all tables.

15. **Design JSON/JSONB columns judiciously.** JSONB in PostgreSQL is powerful but must be used intentionally:
    - **Appropriate uses**: Storing genuinely semi-structured data that varies per record (custom form fields, product attributes that differ by category, third-party webhook payloads, user preferences). Data that is written and read as a unit and rarely queried by internal fields.
    - **Inappropriate uses**: Storing structured data that you regularly filter, join, or aggregate on. If you are writing queries like `WHERE metadata->>'status' = 'active'` frequently, that field should be a proper column.
    - **If using JSONB**: Define the expected schema (even though the database doesn't enforce it — document it and validate in application code). Create GIN indexes on JSONB columns only for fields that are actually queried. Use expression indexes (`CREATE INDEX idx ON table ((data->>'field'))`) for specific field lookups.

16. **Design multi-tenant data architecture (if applicable).** Choose and justify:
    - **Shared tables with tenant_id column** (recommended for most SaaS systems):
      - Add `tenant_id` to every table. Include `tenant_id` in every query's WHERE clause and every index.
      - Use Row-Level Security (RLS) in PostgreSQL to enforce tenant isolation at the database level — prevents accidental cross-tenant data access even if application code omits the tenant filter.
      - Tradeoff: simplest operationally, but noisy neighbor risk (one tenant's heavy queries affect others) and compliance concerns (data is commingled).
    - **Schema-per-tenant** (PostgreSQL schemas):
      - Each tenant gets their own schema with identical table structures. Application sets `search_path` per request.
      - Tradeoff: better isolation, easier per-tenant backup/restore, but schema migrations must be applied to all schemas (operational complexity grows linearly with tenant count). Viable up to ~1000 tenants.
    - **Database-per-tenant**:
      - Maximum isolation, independent scaling, per-tenant backup/restore. Required for strict compliance (HIPAA, certain financial regulations).
      - Tradeoff: highest operational complexity, connection management challenges, migration coordination across all databases. Use only when compliance or customer contracts require it.