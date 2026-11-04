# Phase 12: Reliability and Disaster Recovery

29. **Design file storage reliability.** Object storage is inherently reliable (S3: 99.999999999% durability, 99.99% availability for Standard class), but application-level reliability still requires design:

    **Cross-region replication (CRR)**:
    - Replicate critical files to a secondary region for disaster recovery.
    - **When to use**: Files that cannot be reconstructed (user-uploaded documents, legal records), compliance requirements for geographic redundancy, or active-active multi-region architecture.
    - **Configuration**: S3 Cross-Region Replication rule → specify source bucket, destination bucket (different region), prefix filter (replicate only critical prefixes), and storage class for replicas.
    - **Replication lag**: Typically seconds to minutes. Monitor replication metrics. S3 Replication Time Control (S3 RTC) provides SLA of 15 minutes for 99.99% of objects.
    - **Cost**: Storage in the second region + data transfer between regions. Replicate selectively — replicate original files but not reproducible variants.

    **Versioning for accidental overwrite/delete protection**:
    - Enable S3 versioning on buckets containing important files.
    - If a file is accidentally overwritten or deleted, restore the previous version.
    - Set lifecycle rules to delete old versions after a retention period (30-90 days) to control cost.
    - Combined with MFA Delete (require MFA to permanently delete versions), this provides strong protection against accidental and malicious deletion.

    **Backup strategy for critical files**:
    - S3 itself has 11 nines of durability — individual file backup is generally unnecessary.
    - However, protect against application-level data corruption, accidental mass deletion, or account compromise:
      - Cross-region replication (primary protection).
      - S3 Object Lock in Compliance mode (prevents deletion even by root account for the retention period).
      - Cross-account replication: Replicate to a bucket in a separate AWS account that the primary account cannot modify. Protects against account compromise.
    - For database records that reference files: standard database backup procedures (covered in the database-architecture skill). Ensure database backups and file storage are consistent — a database restore should reference files that exist in S3.

30. **Design file integrity verification.**
    - **Checksum on upload**: Calculate a checksum (SHA-256 or MD5) of the file before/during upload. S3 supports `Content-MD5` for integrity verification during upload, and S3 additional checksums (SHA-256, CRC32) can be stored as object metadata.
    - **Store checksum in database**: Include the checksum in the file metadata record. Use it for deduplication (same checksum = same file, avoid duplicate storage) and integrity verification (periodically verify that the stored checksum matches the S3 object's checksum).
    - **Periodic integrity verification**: For compliance-critical files, periodically (monthly) download a sample of files and verify their checksums against the database records. Alert on any mismatch (indicates corruption or tampering).
    - **Content-addressed storage**: For immutable files (audit logs, compliance records), use the content hash as part of the object key: `audit-logs/sha256/{hash}.json`. This makes tampering detectable and deduplication automatic.
