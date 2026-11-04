### Phase 8: Replication, High Availability, and Disaster Recovery

26. **Design the replication topology.** Based on availability and read-scaling requirements:
    - **Single primary with synchronous replicas** (for HA within a region): The primary replicates to one or more standbys. Synchronous replication ensures zero data loss on failover (RPO = 0) but adds write latency (each write waits for standby acknowledgment). Recommended for data where any loss is unacceptable (financial transactions).
    - **Single primary with asynchronous replicas** (most common): Replicas lag behind the primary by milliseconds to seconds. Provides HA with near-zero data loss and enables read scaling. Recommended as the default for most systems. State the expected replication lag and the consequence: reads from replicas may return slightly stale data.
    - **Read replicas for scaling**: Route read-heavy access patterns (reports, dashboards, search, customer-facing reads that tolerate slight staleness) to replicas. Keep writes on the primary. Define which access patterns can read from replicas and which must read from the primary (e.g., "after creating an order, subsequent reads must use the primary to avoid read-your-own-write inconsistency, or use session affinity to route to the replica that received the write").
    - **Multi-region replication**: For global applications or disaster recovery:
      - **Active-passive**: Primary region handles all writes, secondary region has a replica for failover. RTO depends on failover automation (manual: minutes-hours; automated: seconds-minutes).
      - **Active-active**: Both regions handle writes. Requires conflict resolution strategy (last-write-wins, application-level merge, CRDTs). Significantly more complex. Recommend only when latency requirements demand writes from multiple regions.
    - Define the failover procedure: automatic (managed service handles it) or manual (runbook with steps, responsible team, estimated RTO).

27. **Design the backup and recovery strategy.** Define:
    - **Backup types**:
      - **Automated daily snapshots**: Full database snapshots stored in a separate region/account. Retention: 7-30 days minimum, longer for compliance.
      - **Continuous archiving / Point-in-Time Recovery (PITR)**: WAL archiving (PostgreSQL) or equivalent. Enables recovery to any point in time within the retention window. Recommended for all production databases.
      - **Logical backups** (`pg_dump`, `mongodump`): For portability, selective restore, and cross-version migration. Run periodically in addition to physical backups.
    - **Recovery Point Objective (RPO)**: Maximum acceptable data loss. PITR provides RPO of seconds. Daily snapshots provide RPO of up to 24 hours. Match RPO to business criticality.
    - **Recovery Time Objective (RTO)**: Maximum acceptable downtime during recovery. Define the restore procedure and test it to measure actual RTO.
    - **Backup testing**: Schedule regular restore tests (at least quarterly). An untested backup is not a backup. Restore to a non-production environment and verify data integrity.
    - **Backup security**: Encrypt backups at rest. Store in a separate account or region from the primary database. Restrict access to backup storage.

28. **Design for data durability edge cases.** Address:
    - **Accidental data deletion**: Soft deletes, delayed hard deletes (30-day grace period), or audit logging that captures deleted data.
    - **Accidental schema changes**: Migration rollback procedures, pre-migration backup, and schema change review process.
    - **Corruption**: Checksum verification, replica comparison, and monitoring for replication divergence.