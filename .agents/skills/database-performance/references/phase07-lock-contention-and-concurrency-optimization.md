# Phase 7: Lock Contention and Concurrency Optimization

22. **Diagnose lock contention.** Locks are the primary mechanism for concurrent access control, but excessive locking degrades performance:

    **Identify blocking queries**:
    ```sql
    SELECT
      blocked.pid AS blocked_pid,
      blocked.query AS blocked_query,
      now() - blocked.query_start AS blocked_duration,
      blocking.pid AS blocking_pid,
      blocking.query AS blocking_query,
      now() - blocking.query_start AS blocking_duration
    FROM pg_stat_activity blocked
    JOIN pg_locks blocked_locks ON blocked.pid = blocked_locks.pid AND NOT blocked_locks.granted
    JOIN pg_locks blocking_locks ON blocked_locks.locktype = blocking_locks.locktype
      AND blocked_locks.relation = blocking_locks.relation
      AND blocking_locks.granted
    JOIN pg_stat_activity blocking ON blocking_locks.pid = blocking.pid
    WHERE blocked.pid != blocking.pid;
    ```

    **Identify lock waits**:
    ```sql
    SELECT
      locktype, relation::regclass, mode, granted,
      pid, now() - pg_stat_activity.query_start AS duration,
      query
    FROM pg_locks
    JOIN pg_stat_activity USING (pid)
    WHERE NOT granted
    ORDER BY query_start;
    ```

23. **Resolve common lock contention patterns:**

    **Pattern: DDL blocking DML (and vice versa).** `ALTER TABLE`, `CREATE INDEX` (without CONCURRENTLY), `VACUUM FULL` acquire `ACCESS EXCLUSIVE` locks that block all other operations.
    - Fix: Always use `CREATE INDEX CONCURRENTLY`. Use `ALTER TABLE ... ADD COLUMN` only for non-blocking changes (nullable column with no volatile default). Schedule `VACUUM FULL` only during maintenance windows. Use `lock_timeout` to prevent DDL from waiting indefinitely: `SET lock_timeout = '5s'` — if the lock cannot be acquired in 5 seconds, abort and retry.

    **Pattern: Row-level lock contention.** Multiple transactions updating the same rows simultaneously cause serialization. Common in: counter updates, inventory decrement, status transitions.
    - Fix for counters: Use `UPDATE table SET counter = counter + 1` (atomic increment, minimal lock duration). For high-contention counters, use a sharded counter pattern: maintain N counter rows and SUM them on read.
    - Fix for hot rows: Reduce transaction duration around the hot row — do expensive computation before opening the transaction, then lock and update quickly. Use `SELECT ... FOR UPDATE SKIP LOCKED` for queue-like patterns where any available row is acceptable.
    - Fix for status transitions: Use optimistic concurrency control (`UPDATE orders SET status = 'shipped' WHERE id = ? AND version = ?`) — no explicit lock, retry on conflict.

    **Pattern: Foreign key lock amplification.** When a child row is inserted or deleted, PostgreSQL takes a shared lock on the parent row to verify FK integrity. High-rate child inserts can contend on the parent.
    - Diagnosis: Check for lock waits involving parent tables during child inserts.
    - Fix: Ensure the parent table's PK is indexed (it always is). If contention persists, consider batching child inserts or using deferred FK constraints (`DEFERRABLE INITIALLY DEFERRED`).

    **Pattern: Deadlocks.** Two or more transactions waiting for each other's locks.
    - Diagnosis: PostgreSQL automatically detects and terminates one deadlocking transaction (logged in the server log). Monitor deadlock count via `pg_stat_database.deadlocks`.
    - Fix: Ensure all code paths acquire locks in a consistent order (e.g., always lock resources in ascending ID order). Reduce transaction scope to minimize the window for deadlocks. Add retry logic for deadlock errors (SQLSTATE '40P01').

24. **Optimize advisory lock usage.** For application-level coordination:
    - Use PostgreSQL advisory locks (`pg_advisory_lock`, `pg_try_advisory_lock`) for distributed synchronization (e.g., ensuring only one worker processes a specific job).
    - **Always use `pg_try_advisory_lock`** (non-blocking) rather than `pg_advisory_lock` (blocking) unless you specifically need to queue. A blocked advisory lock holds a connection.
    - Release advisory locks explicitly (`pg_advisory_unlock`). Session-level advisory locks are released when the connection is returned to the pool in transaction pooling mode — this can cause unexpected behavior with PgBouncer. Use transaction-level advisory locks (`pg_advisory_xact_lock`) that auto-release at transaction end.
