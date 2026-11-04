# Phase 3: Bucket and Key Architecture

7. **Design the bucket architecture.** Buckets (S3) / containers (Azure) / buckets (GCS) are the top-level organizational unit for object storage:

   **Bucket design principles**:
   - **Separate buckets by access pattern and security boundary**, not by file type alone:
     - `myapp-public-assets` — publicly readable files (product images, marketing assets). Bucket policy allows public read.
     - `myapp-private-uploads` — private user-uploaded files. No public access. Access via presigned URLs.
     - `myapp-internal-data` — internal application data (backups, exports, processing artifacts). No external access.
     - `myapp-compliance-archive` — compliance-sensitive files with Object Lock and restricted deletion policies.
   - **Separate buckets by environment**: `myapp-dev-uploads`, `myapp-staging-uploads`, `myapp-prod-uploads`. Never share buckets across environments — a misconfigured policy in dev could expose production data.
   - **Separate buckets by region** when data residency requirements apply: `myapp-uploads-eu`, `myapp-uploads-us`. Files for EU customers stay in EU buckets.
   - **Do not create a bucket per user or per tenant** (in most cases): S3 has a soft limit of 100 buckets per account (increasable to 1000). Use key prefixes for tenant isolation within a shared bucket: `myapp-uploads/tenant-123/files/...`. If strict bucket-level isolation is required (compliance, separate billing), use separate AWS accounts per tenant.

   **Bucket configuration**:
   - **Block public access**: Enable "Block Public Access" by default on ALL buckets. Disable only on the specific bucket that needs public access (public assets), and only after explicit security review.
   - **Versioning**: Enable on buckets where accidental overwrite or deletion must be recoverable (user uploads, compliance archives). Versioning retains all previous versions of an object. Note: versioning increases storage costs (old versions are retained). Set lifecycle rules to delete old versions after a retention period.
   - **Server-side encryption**: Enable by default on all buckets (step 17).
   - **Logging**: Enable S3 server access logging or CloudTrail data events for buckets containing sensitive files (step 33).
   - **Object Lock** (for compliance): Enable on buckets where files must not be modified or deleted for a retention period (WORM — Write Once Read Many). Required for SEC 17a-4, HIPAA, and similar regulations. Object Lock modes: Governance (admin can override), Compliance (nobody can override, not even root).

8. **Design the object key (path) structure.** The object key is the unique identifier for each file within a bucket. Key design affects performance, organization, listing operations, and operational management:

   **Key structure conventions**:
   ```
   {tenant_or_scope}/{entity_type}/{entity_id}/{purpose}/{filename_or_hash}
   ```

   **Examples**:
   ```
   users/usr_abc123/avatar/original/a1b2c3d4.jpg
   users/usr_abc123/avatar/thumb-128/a1b2c3d4.jpg
   users/usr_abc123/documents/doc_xyz/invoice-2024-01.pdf
   products/prod_456/images/original/front-view.jpg
   products/prod_456/images/webp-800/front-view.webp
   orders/ord_789/receipts/receipt-2024-01-15.pdf
   exports/tenant-123/2024/01/daily-report-2024-01-15.csv
   backups/db/2024/01/15/mydb-20240115-1030.sql.gz
   tmp/upload-sessions/session_abc/chunk-001
   ```

   **Key design rules**:
   - **Use system-generated filenames, not user-provided filenames**: Store the original filename in the database metadata record. The object key uses a UUID, hash, or system-generated identifier: `users/usr_abc/avatar/a1b2c3d4e5f6.jpg`, not `users/usr_abc/avatar/my photo (final) (2).jpg`. This prevents: path traversal attacks, special character encoding issues, filename collisions, and excessively long keys.
   - **Include the file extension** in the key: Helps with content type inference, debugging, and manual inspection. `a1b2c3.jpg`, not just `a1b2c3`.
   - **Use forward slashes as logical separators**: S3 treats `/` as a delimiter for "folder-like" listing operations (`ListObjectsV2` with `Delimiter=/`).
   - **Include a variant/purpose segment** for transformed files: `original/`, `thumb-128/`, `webp-800/`, `hls/`. This keeps all variants of a file organized together.
   - **Include date-based prefixes for time-series files**: `backups/2024/01/15/`, `exports/2024/01/`. Enables efficient listing and lifecycle rules by prefix.
   - **Key length**: Keep keys under 1024 bytes (S3 maximum). Practically, keep them under 256 characters for readability.
   - **Character set**: Use alphanumeric characters, hyphens, underscores, forward slashes, and dots. Avoid spaces, special characters, and non-ASCII characters in keys.

   **S3 performance and key design**:
   - S3 supports at least 5,500 GET/HEAD and 3,500 PUT/COPY/POST/DELETE requests per second per prefix. For extremely high-throughput workloads (> 5,000 requests/second to the same prefix), add randomized prefixes to distribute load across S3 partitions: `a1/users/...`, `b2/users/...`. In practice, S3 automatically partitions for high-throughput workloads, and this optimization is rarely needed for most applications.
