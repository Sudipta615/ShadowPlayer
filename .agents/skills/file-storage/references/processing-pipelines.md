# Phase 8: File Processing Pipelines

20. **Design the file processing architecture.** Many file storage systems require processing files after upload (image resizing, video transcoding, document conversion, virus scanning):

    **Event-driven processing pipeline** (recommended):
    ```
    S3 Upload → S3 Event Notification → SQS Queue → Processing Worker → Store Results → Update Database
    ```
    - **S3 Event Notification**: Configure S3 to send events (`s3:ObjectCreated:*`) for the relevant prefix (e.g., `uploads/pending/*`) to SQS, SNS, Lambda, or EventBridge.
    - **SQS queue**: Buffer processing requests. Decouple upload rate from processing rate. Handle bursts gracefully.
    - **Processing worker**: Consumes from SQS, performs the file transformation, stores results in S3, and updates the database.
    - **Advantages**: Decoupled (uploads are not blocked by processing), scalable (add more workers for more throughput), fault-tolerant (failed processing is retried via SQS visibility timeout and DLQ).

    **Lambda-based processing** (for lightweight, fast transformations):
    - S3 Event → Lambda function → process file → store result → update database.
    - **Advantages**: No infrastructure to manage. Auto-scales with upload volume. Pay only for processing time.
    - **Constraints**: Lambda has a 15-minute maximum execution time, 10GB memory limit, and limited CPU. Suitable for: image resizing (< 50MP images), PDF generation (< 100 pages), virus scanning (< 500MB files), metadata extraction.
    - **Not suitable for**: Video transcoding (too slow for Lambda), large file processing (> 5GB), complex multi-step pipelines.

    **Dedicated processing service** (for heavy processing):
    - For video transcoding, large document processing, or complex multi-step pipelines, use dedicated processing infrastructure:
      - **AWS MediaConvert**: Managed video transcoding. Supports HLS/DASH output, multiple resolutions, codec conversion.
      - **FFmpeg on ECS/EKS**: Self-managed video/audio processing. More control, more operational complexity.
      - **Custom workers on EC2/ECS**: For CPU/GPU-intensive processing (ML-based content moderation, OCR, image recognition).

21. **Design processing status tracking.** Consumers need to know when processing is complete:

    **Polling-based status check**:
    - Client polls `GET /api/files/{file_id}` to check status. Response includes `status: 'processing'` or `status: 'completed'` with variant URLs.
    - Simple, but wasteful if polling is frequent.

    **Webhook/event notification**:
    - Backend sends a webhook or pushes a notification (WebSocket, SSE) to the client when processing completes.
    - More efficient than polling, but requires the client to handle async notifications.

    **Optimistic display**:
    - For image uploads, display the original uploaded image immediately while processing (resizing, optimization) happens in the background. When variants are ready, the client can switch to the optimized version on next page load.
    - Best user experience — the user sees their upload immediately.

22. **Design orphaned file cleanup.** Files can become orphaned (stored in S3 but not referenced by any database record) through: failed upload completions, bugs in the upload flow, deleted database records without corresponding S3 deletion, or abandoned upload sessions.

    - **Prevention**: Use the transactional approach — create the database record and the S3 object together. If the database write fails, delete the S3 object. If the S3 write fails, mark the database record as failed.
    - **Detection**: Run a periodic reconciliation job that compares S3 objects against database records. Objects in S3 that have no corresponding database record are orphaned.
    - **Cleanup**: Delete orphaned objects after a grace period (7-30 days — to allow for in-flight uploads and recent deletions). Log every deletion for audit purposes.
    - **S3 lifecycle rules for pending uploads**: Set a lifecycle rule to delete objects in the `uploads/pending/` prefix that are older than 24-72 hours. These are abandoned upload sessions.
    - **S3 lifecycle rules for incomplete multipart uploads**: Set `AbortIncompleteMultipartUpload` lifecycle rule with `DaysAfterInitiation: 7`. Incomplete multipart uploads consume storage but are not visible as objects. This rule cleans them up automatically.
