### Phase 12: Data Lifecycle, Retention, and Archival

39. **Design data retention policies.** For each entity:
    - **Define the retention period**: How long must data be kept in the primary database? Base this on: business requirements (how far back do users need to access data?), compliance requirements (GDPR: data minimization; financial regulations: 7-year retention), and operational requirements (database performance degrades with unconstrained growth).
    - **Define the archival strategy**:
      - **Partition-based archival**: Detach old partitions and move to cold storage (cheaper database instance, S3, or data lake). Old partitions can be re-attached if historical access is needed.
      - **Tiered storage**: Move old data from hot (SSD) to warm (HDD) to cold (object storage) based on access frequency.
      - **Archive tables**: Move old records to archive tables within the same database. Simpler but doesn't reduce database size.
    - **Define the purging strategy**: For data that must be permanently deleted (GDPR right to erasure):
      - Design the deletion procedure: cascade effects, referential integrity handling, audit trail of deletion.
      - For large-scale deletions, batch the deletes to avoid lock contention and replication lag: delete in chunks of 1000-10000 rows with a pause between batches.
    - **Implement TTL where supported**: DynamoDB TTL, Redis TTL, Cassandra TTL — use database-native TTL for automatic expiration of ephemeral data (sessions, tokens, temporary locks).

40. **Design data anonymization and pseudonymization (for compliance).** When regulations require:
    - **Anonymization**: Irreversibly remove identifying information. Used for analytics datasets. Define which fields are anonymized and the technique (hashing, generalization, suppression, noise addition).
    - **Pseudonymization**: Replace identifying information with reversible pseudonyms. The mapping is stored separately and protected. Enables re-identification when necessary (e.g., for customer support) while protecting data at rest.
    - **Implement per-user data export and deletion**: Design the query and procedure for extracting or deleting all data associated with a specific user across all tables and services. This is a GDPR requirement and must be tested.