# Phase 13: Replication Lag Performance

36. **Diagnose and resolve replication lag.** When replication lag impacts application behavior or data freshness:

    **Diagnose the cause**:
    - **Replica under-resourced**: The replica has less CPU, memory, or IOPS than the primary. Replays WAL slower than the primary generates it. Fix: size replicas at least as large as the primary.
    - **Long-running queries on the replica**: Queries on the replica can conflict with WAL replay, causing replay to pause. In PostgreSQL, `hot_standby_feedback` and `max_standby_streaming_delay` control this behavior.
      - `max_standby_streaming_delay`: How long replay waits for a conflicting query before cancelling the query. Default: 30s. Set lower (5-10s) if replay timeliness is more important than query completion.
      - `hot_standby_feedback`: When `on`, the replica informs the primary about its oldest active query, preventing the primary from vacuuming rows the replica still needs. This prevents query cancellation but can cause bloat on the primary if replica queries are long-running.
    - **Network bandwidth**: WAL streaming saturates the network between primary and replica. Diagnosis: compare WAL generation rate to network throughput. Fix: use WAL compression (`wal_compression = on`), or improve network bandwidth.
    - **High write volume**: The primary generates WAL faster than the replica can replay. Fix: ensure replica has sufficient I/O capacity. Consider `max_parallel_apply_workers` (logical replication) or accepting that physical replication replay is single-threaded (PostgreSQL limitation — consider using streaming replication with a higher-spec replica).

    **Mitigate lag impact**:
    - Route time-sensitive reads to the primary.
    - Implement lag-aware routing: monitor replica lag and remove replicas with lag > threshold from the read pool.
    - For analytical replicas, accept higher lag (minutes to hours) and use a dedicated replica that is not in the OLTP read pool.
