# Phase 14: File Storage Observability

32. **Design file storage monitoring metrics.**

    **Upload metrics**:
    - Upload request rate (presigned URL generations per second).
    - Upload completion rate (successful uploads per second).
    - Upload failure rate (abandoned or rejected uploads per second).
    - Upload latency (time from URL generation to upload completion).
    - Upload size distribution (histogram).
    - Pending uploads (uploads in `pending` status for > 1 hour — potential stuck uploads).
    - Virus scan results (clean vs. infected vs. error per day).

    **Download metrics**:
    - Download request rate (presigned URL generations and CDN requests per second).
    - Download latency (time to first byte from S3 and CDN).
    - CDN hit rate for file serving (should be > 90% for public assets).
    - Bandwidth consumption (GB/day, trending).

    **Storage metrics**:
    - Total storage by bucket, prefix, and storage class (use S3 Storage Lens or CloudWatch metrics).
    - Storage growth rate (GB/day, GB/month).
    - Object count by prefix and status.
    - Orphaned file count (detected by reconciliation job).
    - Lifecycle transition count (objects moved between storage classes per day).

    **Processing metrics**:
    - Processing queue depth (SQS messages waiting).
    - Processing latency (time from upload to processing completion).
    - Processing error rate.
    - Processing throughput (files processed per second).

    **Cost metrics**:
    - Daily/monthly S3 cost by category (storage, requests, transfer).
    - Cost per tenant (if applicable).
    - Cost trend and projection.

33. **Design file storage audit logging.** For compliance and security:

    **S3 server access logging**:
    - Enable on all buckets containing sensitive files. Logs every S3 API call: who accessed what, when, from where, with what result.
    - Store access logs in a separate, dedicated logging bucket (not the same bucket — this creates an infinite loop).
    - Retention: 1-7 years depending on compliance requirements.

    **CloudTrail data events**:
    - Enable CloudTrail data events for S3 for more structured, queryable audit logs.
    - More expensive than server access logging but integrates with CloudTrail analysis tools, SIEM, and EventBridge.
    - Use for compliance-critical buckets (healthcare records, financial documents).

    **Application-level audit logging**:
    - Log file operations in the application's audit log:
      - File uploaded: who, when, file category, file size.
      - File accessed: who, when, file ID, access type (view, download).
      - File deleted: who, when, file ID, deletion type (soft, hard, GDPR).
      - File shared: who shared, with whom, file ID, permissions granted.
    - Store audit logs separately from application data. Ensure they are tamper-resistant.

34. **Design file storage alerting.**

    **Critical (page)**:
    - Upload presigned URL generation failing (users cannot upload).
    - S3 bucket returning 5xx errors.
    - Processing queue depth growing for > 15 minutes (processing pipeline stalled).
    - Virus scan detecting infected files (potential attack or compromised user).
    - Storage approaching account/bucket limits.
    - Unexpected public access detected on private bucket (security incident).

    **Warning (ticket)**:
    - Upload completion rate < 80% of initiation rate for > 1 hour (abandoned uploads, potential UX issue).
    - Processing latency exceeding SLA for > 30 minutes.
    - CDN hit rate dropping below 80% (cache configuration issue).
    - Storage cost exceeding budget threshold.
    - Orphaned file count increasing (reconciliation job detecting drift).
    - High rate of access denied errors (potential misconfiguration or unauthorized access attempts).
