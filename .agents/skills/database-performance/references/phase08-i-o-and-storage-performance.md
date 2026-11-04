# Phase 8: I/O and Storage Performance

25. **Diagnose I/O bottlenecks.** I/O is the fundamental bottleneck for most database workloads. When CPU is idle but queries are slow, I/O is almost always the cause.

    **Check I/O wait**: Monitor `iowait` percentage. > 10% sustained indicates I/O pressure.

    **Check disk throughput and IOPS**: Compare current utilization to the storage's provisioned limits. For cloud storage:
    - AWS EBS gp3: 3,000 baseline IOPS, 125 MB/s throughput (configurable up to 16,000 IOPS, 1,000 MB/s).
    - AWS EBS io2: Up to 64,000 IOPS per volume.
    - If you're hitting the IOPS limit, provision more IOPS or move to a higher-performance storage tier.

    **Check which operations cause I/O**:
    ```sql
    SELECT
      queryid, calls,
      shared_blks_read AS blocks_read_from_disk,
      shared_blks_hit AS blocks_from_cache,
      temp_blks_read + temp_blks_written AS temp_disk_blocks,
      query
    FROM pg_stat_statements
    ORDER BY shared_blks_read DESC
    LIMIT 20;
    ```
    Queries with high `shared_blks_read` are the primary I/O consumers. Optimize these queries (better indexes, less data scanned) or increase cache to reduce reads.

26. **Optimize storage configuration.** Targeted storage optimizations:

    **Use SSD storage for all production databases.** This is non-negotiable. The difference between HDD and SSD for random I/O (the dominant pattern for indexed queries) is 100-1000x.

    **Separate WAL from data storage** (for self-managed databases): Place WAL on a separate volume/disk to avoid WAL writes contending with data reads. On managed services, this is handled automatically.

    **Configure filesystem**: Use `ext4` or `xfs` with `noatime` mount option (disables access time updates, reduces I/O). For XFS, use the default settings — they are well-tuned for database workloads.

    **Tablespace placement** (for extreme optimization): Place hot tables and indexes on the fastest storage, cold/archival data on cheaper storage using PostgreSQL tablespaces. This is rarely necessary with modern SSDs.

    **TOAST configuration**: PostgreSQL automatically compresses and out-of-lines large column values (> 2KB) into TOAST tables. For tables with many large text/JSONB columns, TOAST can cause I/O amplification. Monitor TOAST table sizes. Consider storing large values in object storage (S3) with a reference in the database.

27. **Optimize checkpoint I/O.** Checkpoints flush all dirty buffers to disk, causing I/O spikes:
    - **Diagnosis**: Check PostgreSQL logs for checkpoint timing: `LOG: checkpoint complete: wrote X buffers (Y%); ... write=Z s`. If checkpoints are frequent (every few minutes) and write many buffers, they are causing I/O spikes.
    - **Fix**: Increase `max_wal_size` to allow more WAL between checkpoints (reduces checkpoint frequency). Ensure `checkpoint_completion_target = 0.9` to spread writes. Monitor that checkpoints complete within the configured interval.
    - **`full_page_writes`**: Leave `on` (required for crash recovery). But understand it doubles write volume after each checkpoint (every page is written in full the first time it's modified after a checkpoint). This is why reducing checkpoint frequency also reduces total I/O.
