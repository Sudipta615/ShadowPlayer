# Phase 15: File Migration

35. **Design file migration strategies.** For migrating files between storage systems, providers, or architectures:

    **Migration between storage providers** (e.g., Azure Blob → S3):
    - **Inventory**: List all files in the source system with metadata (key, size, content type, created date). Generate a migration manifest.
    - **Transfer**: Use a migration tool: AWS DataSync, rclone (open-source, multi-cloud), cloud-native transfer services (Storage Transfer Service for GCS). For very large migrations (petabytes), use offline transfer (AWS Snowball).
    - **Verification**: After transfer, verify: file count matches, checksums match for a statistically significant sample, and metadata is preserved.
    - **Cutover**: Update the application to read from the new storage. Dual-read during migration: try new storage first, fall back to old storage if not found (for files not yet migrated).
    - **Cleanup**: After migration is complete and verified, delete files from the source storage.

    **Migration from local file system / NFS to object storage**:
    - Map file system paths to object keys: `/data/users/123/avatar.jpg` → `users/usr_123/avatar/original/avatar.jpg`.
    - Update all file path references in the database.
    - Update application code to use object storage APIs instead of file system APIs.
    - Run both systems in parallel during migration (dual-read/dual-write) for safety.

    **Migration from database BLOBs to object storage**:
    - Extract BLOBs from the database, upload to S3 with generated keys, and replace the BLOB column with a `storage_key` reference column.
    - Migrate in batches to avoid database lock contention.
    - Verify each batch by comparing checksums.
    - After migration, drop the BLOB column to reclaim database space.
