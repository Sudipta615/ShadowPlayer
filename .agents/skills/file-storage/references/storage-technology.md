# Phase 2: Storage Technology Selection

5. **Select the storage technology.** Match the technology to the file access patterns and requirements:

   **Object storage (S3, GCS, Azure Blob Storage)** — recommended as the default for almost all file storage:
   - **What it is**: Flat namespace storage where each object is identified by a unique key within a bucket/container. Objects are immutable (replaced, not modified in-place). Designed for HTTP access, virtually unlimited scale, and 99.999999999% (11 nines) durability.
   - **Select when**: Virtually all file storage scenarios — user uploads, media assets, documents, backups, archives, application-generated files. This is the default answer unless a specific requirement disqualifies it.
   - **AWS S3** (default recommendation on AWS): Broadest feature set, widest ecosystem integration, most mature lifecycle management, storage class variety, event notifications, S3 Select for in-place querying, S3 Object Lock for compliance.
   - **Google Cloud Storage** (default on GCP): Unified API across storage classes, strong consistency, integrated with GCP services.
   - **Azure Blob Storage** (default on Azure): Hot/Cool/Archive tiers, Azure CDN integration, lifecycle management.
   - **MinIO** (self-hosted, S3-compatible): When you need S3-compatible storage on-premise, in a private cloud, or in development/testing environments. Production self-hosting requires operational expertise.
   - **Advantages**: Virtually unlimited scale, extremely high durability, HTTP-native, rich lifecycle management, integrates with CDN, event notifications, no file system semantics to manage, pay-per-use pricing.
   - **Disadvantages**: Higher latency than local file systems for individual small file operations (10-100ms per request vs. < 1ms for local disk), no append or partial update (objects are immutable — must rewrite entire object to modify), eventual consistency for some operations in some configurations (S3 is now strongly consistent for all operations as of December 2020).

   **Managed file systems (EFS, FSx, Filestore, Azure Files)**:
   - **Select when**: Applications require POSIX file system semantics (random read/write within files, file locking, directory listing), shared file system access from multiple compute instances simultaneously, legacy applications that expect a mounted file system.
   - **EFS (Elastic File System, AWS)**: NFS-compatible, automatically scales, multi-AZ. Good for shared configuration, content management systems, or applications that require a shared mounted file system.
   - **FSx for Lustre/NetApp/Windows**: High-performance file systems for specific workloads (HPC, Windows workloads, media processing).
   - **When NOT to select**: For simple file upload/download/serve use cases — object storage is simpler, cheaper, and more scalable. Do not use managed file systems just because the application "needs to store files."

   **Block storage (EBS, Persistent Disks, Azure Managed Disks)**:
   - **Not for file storage**: Block storage provides raw disk volumes attached to compute instances. Use for database storage, OS disks, and application state — not for serving user files. Files stored on block storage are only accessible from the attached instance and are not HTTP-addressable.

   **Database BLOB storage** — **do not use for most file storage**:
   - Storing files as BLOBs in a relational database is almost always wrong:
     - Increases database size dramatically, slowing backups and replication.
     - Database memory is consumed by file data, reducing cache effectiveness for actual query data.
     - Files cannot be served directly via HTTP/CDN — they must be read from the database and proxied through the application.
     - Database connections are held during file transfer, reducing connection pool availability.
   - **Acceptable only when**: Files are extremely small (< 256KB), tightly coupled to database transactions (the file must be atomically committed with related data), and the file count is small. Even then, consider storing a reference (URL/key) in the database and the file in object storage.
   - **Recommended pattern**: Store file metadata (name, size, content type, storage key, upload timestamp, owner) in the database. Store the file bytes in object storage. The database record's `storage_key` field links to the object storage key.

6. **Select storage classes / tiers.** Object storage providers offer multiple storage classes with different cost-performance tradeoffs. Match each file category to the appropriate class:

   **AWS S3 storage classes** (GCS and Azure have analogous tiers):
   - **S3 Standard**: Default. Low latency, high throughput. Use for frequently accessed files (< 30 days since upload or with regular access). Most of your "active" files live here.
   - **S3 Intelligent-Tiering**: Automatically moves objects between access tiers based on access patterns. Use when access patterns are unpredictable. No retrieval fees. Small monitoring fee per object. Recommended when you cannot predict whether files will be frequently or infrequently accessed.
   - **S3 Standard-Infrequent Access (S3 Standard-IA)**: Lower storage cost, per-retrieval fee. Use for files accessed less than once per month but requiring immediate access when needed. Minimum storage duration: 30 days. Minimum object size charge: 128KB.
   - **S3 One Zone-IA**: Same as Standard-IA but stored in a single AZ. Lower cost, lower durability (99.5% availability vs. 99.9%). Use for easily reproducible files (thumbnails, transcoded videos, cached transformations).
   - **S3 Glacier Instant Retrieval**: Very low storage cost, higher retrieval cost, millisecond retrieval. Use for files accessed approximately once per quarter (archival with occasional access).
   - **S3 Glacier Flexible Retrieval**: Very low storage cost, minutes-to-hours retrieval time. Use for archival data accessed 1-2 times per year. Retrieval options: Expedited (1-5 minutes), Standard (3-5 hours), Bulk (5-12 hours).
   - **S3 Glacier Deep Archive**: Lowest storage cost, 12-48 hour retrieval time. Use for long-term compliance archives accessed less than once per year.

   **Storage class assignment per file category** (example):
   | Category | Initial Class | Transition After 30 Days | Transition After 90 Days | Transition After 1 Year |
   |---|---|---|---|---|
   | Profile photos | Standard | — | — | — (always accessed) |
   | Product images | Standard | — | — | — (always accessed) |
   | Invoices | Standard | Standard-IA | Glacier Instant | Glacier Flexible |
   | User uploads | Standard | Intelligent-Tiering | — (auto-managed) | — |
   | Database backups | Standard-IA | Glacier Flexible | Deep Archive | Delete after 90 days |

   Define lifecycle rules to automate transitions (step 30).
