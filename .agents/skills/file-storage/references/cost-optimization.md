# Phase 11: Cost Optimization

27. **Analyze and optimize storage costs.** Storage costs can grow unexpectedly. Design proactive cost management:

    **Cost components** (S3 pricing model):
    - **Storage**: Per-GB-month, varies by storage class. Standard: ~$0.023/GB/month. Standard-IA: ~$0.0125/GB/month. Glacier: ~$0.004/GB/month.
    - **Requests**: Per-request charges. PUT/POST: ~$0.005 per 1,000. GET: ~$0.0004 per 1,000. Significant at high request volumes.
    - **Data transfer**: Free ingress (upload). Egress (download) to internet: ~$0.09/GB (first 10TB/month). Egress to CloudFront: $0.00 (free). Transfer between S3 and services in the same region: $0.00. **This is why CDN is cost-effective** — serving files through CloudFront eliminates S3 egress charges.
    - **Lifecycle transition**: Per-object transition charge when moving between storage classes.
    - **Retrieval**: Per-GB retrieval charges for IA, Glacier, and Deep Archive classes.

    **Cost optimization strategies**:

    **Strategy 1: Right-size storage classes** (largest impact):
    - Use S3 Storage Class Analysis (or S3 Analytics) to identify objects that have not been accessed in 30+ days and could be moved to a cheaper class.
    - Implement lifecycle rules to automatically transition files (step 24).
    - Use Intelligent-Tiering for files with unpredictable access patterns — it automatically moves objects to the optimal tier.

    **Strategy 2: Serve through CDN** (reduces transfer costs):
    - S3 to CloudFront transfer is free. CloudFront to internet is cheaper than S3 to internet ($0.085/GB vs. $0.09/GB for first 10TB, and much cheaper at higher tiers).
    - For read-heavy workloads, CDN caching dramatically reduces both S3 request costs (fewer origin requests) and transfer costs.

    **Strategy 3: Delete unused files** (reduces storage costs):
    - Implement orphaned file cleanup (step 22).
    - Enforce retention policies (step 23).
    - Delete old file versions (lifecycle rule for `NoncurrentVersionExpiration`).
    - Abort incomplete multipart uploads (lifecycle rule for `AbortIncompleteMultipartUpload`).

    **Strategy 4: Compress before storing** (reduces storage and transfer costs):
    - For text-based files (CSV, JSON, XML, logs), compress (gzip, zstd) before uploading. Set `Content-Encoding: gzip` so clients decompress transparently.
    - For images, optimize quality/size tradeoff during processing (JPEG quality 80 instead of 100, WebP instead of PNG).
    - For video, select efficient codecs (H.265/HEVC for storage efficiency, H.264 for compatibility).

    **Strategy 5: Use S3 One Zone-IA for reproducible files**:
    - File variants (thumbnails, transcoded videos, WebP conversions) can be regenerated from the original. Store them in One Zone-IA (~20% cheaper than Standard-IA) since losing them is recoverable.

    **Strategy 6: Request optimization**:
    - Batch `ListObjects` calls (use pagination efficiently, cache listing results).
    - Use `HEAD` instead of `GET` when you only need metadata.
    - Avoid unnecessary `PUT` requests (don't overwrite an object if it hasn't changed — check the ETag first).

28. **Design cost monitoring and alerting.**
    - Set up AWS Cost Explorer or Budget alerts for S3 spending. Alert when daily/monthly spending exceeds expected thresholds.
    - Track storage cost per tenant, per file category, and per storage class. Build a cost dashboard:
      - Total storage by class (GB).
      - Total requests by type (PUT, GET, LIST).
      - Total transfer by destination (internet, CloudFront, cross-region).
      - Cost trend over time (daily, monthly).
      - Storage growth rate and projected cost at current growth.
    - Identify cost anomalies: sudden spike in request count (possible scraping or abuse), unexpected storage growth (file upload bug creating excessive variants), or high retrieval costs from Glacier (inappropriate access pattern for archived data).
