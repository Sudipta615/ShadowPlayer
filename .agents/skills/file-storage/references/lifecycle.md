# Phase 9: Storage Lifecycle Management

23. **Design retention policies.** Every file category must have a defined retention period. Unbounded retention leads to unbounded cost and compliance risk:

    | Category | Retention | After Retention | Compliance Basis |
    |---|---|---|---|
    | Profile photos | Account lifetime + 30 days | Delete | User data policy |
    | Product images | Product lifetime + 90 days | Delete | Business policy |
    | Invoices | 7 years | Archive to Glacier, delete after 10 years | Financial regulations |
    | User uploads | 1 year after last access | Delete (with 30-day notice) | Terms of service |
    | Session recordings | 90 days | Delete | Privacy policy |
    | Database backups | 90 days | Delete | DR policy |
    | Audit logs | 7 years | Archive to Deep Archive | SOC 2, regulatory |
    | Compliance documents | Indefinite (with legal hold) | Do not delete | Regulatory |
    | Temporary/processing files | 24-72 hours | Delete | Operational |

24. **Design S3 lifecycle rules.** Automate storage class transitions and expiration:

    ```xml
    <!-- Lifecycle rule: Invoices → Standard-IA after 30 days → Glacier after 90 days → Delete after 10 years -->
    <LifecycleConfiguration>
      <Rule>
        <ID>invoice-lifecycle</ID>
        <Filter><Prefix>invoices/</Prefix></Filter>
        <Status>Enabled</Status>
        <Transition>
          <Days>30</Days>
          <StorageClass>STANDARD_IA</StorageClass>
        </Transition>
        <Transition>
          <Days>90</Days>
          <StorageClass>GLACIER</StorageClass>
        </Transition>
        <Expiration>
          <Days>3650</Days>
        </Expiration>
      </Rule>
      
      <!-- Clean up incomplete multipart uploads -->
      <Rule>
        <ID>abort-incomplete-multipart</ID>
        <Filter><Prefix/></Filter>
        <Status>Enabled</Status>
        <AbortIncompleteMultipartUpload>
          <DaysAfterInitiation>7</DaysAfterInitiation>
        </AbortIncompleteMultipartUpload>
      </Rule>
      
      <!-- Clean up pending uploads -->
      <Rule>
        <ID>clean-pending-uploads</ID>
        <Filter><Prefix>uploads/pending/</Prefix></Filter>
        <Status>Enabled</Status>
        <Expiration>
          <Days>3</Days>
        </Expiration>
      </Rule>
      
      <!-- Delete old versions after 30 days (for versioned buckets) -->
      <Rule>
        <ID>clean-old-versions</ID>
        <Filter><Prefix/></Filter>
        <Status>Enabled</Status>
        <NoncurrentVersionExpiration>
          <NoncurrentDays>30</NoncurrentDays>
        </NoncurrentVersionExpiration>
      </Rule>
    </LifecycleConfiguration>
    ```

    - Define lifecycle rules in infrastructure-as-code (Terraform), not manually in the console.
    - Test lifecycle rules in a non-production environment first. A misconfigured rule can delete production data.
    - Monitor lifecycle transitions: track the number of objects transitioned and expired per rule.

25. **Design file deletion.** File deletion is more complex than it appears due to: database records, object storage, CDN cache, file variants, entity associations, and backups.

    **Soft delete flow**:
    1. Application sets `deleted_at` on the file record in the database.
    2. File is no longer returned in API responses.
    3. File remains in S3 (the user can be offered an "undo" period).
    4. After the undo period (7-30 days), a background job permanently deletes:
       - The S3 object (and all variants).
       - The database record (hard delete) or mark as permanently deleted.
       - CDN cache invalidation (if the file was served via CDN).
    5. Log the deletion for audit purposes.

    **GDPR right to erasure**:
    - When a user requests data deletion, all files owned by or containing PII of that user must be identified and deleted.
    - This includes: files in the primary bucket, all variants (thumbnails, transcoded versions), files in archive/glacier storage (restoration + deletion required), references in backups (accept that backup retention handles this — document the policy).
    - Generate a deletion confirmation record: what was deleted, when, and by whose request.
    - CDN caches must be purged for deleted files.

    **Legal hold override**:
    - If a file is under legal hold (S3 Object Lock), it cannot be deleted even by admin. Legal holds must be explicitly removed before deletion. Document the process for releasing legal holds.
