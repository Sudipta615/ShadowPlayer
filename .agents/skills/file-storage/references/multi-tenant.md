# Phase 10: Multi-Tenant File Storage

26. **Design multi-tenant file isolation.** For SaaS applications serving multiple tenants:

    **Prefix-based isolation** (recommended for most SaaS):
    - All tenants share a bucket, but files are organized under tenant-specific prefixes: `tenants/{tenant_id}/uploads/...`.
    - Application-level enforcement: Every file operation must include the tenant context. Validate that the requesting tenant matches the file's tenant prefix.
    - IAM cannot enforce per-prefix access within a single bucket (S3 bucket policies support prefix conditions, but not per-request tenant context from the application). Enforcement is in the application code.
    - **Risk**: A bug in the application code could expose Tenant A's files to Tenant B. Mitigate with: thorough authorization testing, code review for tenant-scoped file access, and integration tests that verify cross-tenant access is denied.

    **Bucket-per-tenant** (for strict isolation requirements):
    - Each tenant gets their own S3 bucket: `myapp-tenant-123-uploads`.
    - IAM policies scope each tenant's service credentials to their bucket only.
    - **Advantages**: Strongest isolation. Per-tenant billing is trivial. Per-tenant bucket policies. Independent lifecycle rules.
    - **Disadvantages**: Bucket management overhead (create/configure/delete buckets as tenants onboard/offboard). S3 account limit of 100-1000 buckets. More complex application code (dynamic bucket selection).
    - **When to use**: When compliance requires physical isolation (e.g., tenant contractually requires their data to be in a separate container), or when the tenant count is small (< 100).

    **AWS account-per-tenant** (maximum isolation):
    - Each tenant gets their own AWS account with their own S3 buckets. Use AWS Organizations for management.
    - **When to use**: When regulatory requirements demand complete infrastructure isolation (specific HIPAA or government scenarios), or when tenants require independent encryption keys.

    **Storage quota enforcement**:
    - Track storage usage per tenant in the database (increment on upload, decrement on delete). Enforce quotas before generating presigned upload URLs.
    - Periodically reconcile the tracked usage with actual S3 usage (S3 Inventory or `ListObjectsV2` with prefix) to detect drift.
