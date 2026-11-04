# Phase 13: File Storage API Design

31. **Design the file management API.** The application's file management API mediates between clients and object storage:

    **Upload flow API**:
    ```
    POST /api/files/upload-url
    Request: { "filename": "invoice.pdf", "content_type": "application/pdf", "size": 524288, "category": "document", "entity_type": "order", "entity_id": "ord_123" }
    Response: { "upload_id": "upl_abc", "upload_url": "https://s3...presigned-url", "expires_at": "2024-01-15T11:00:00Z" }
    ```

    **Upload completion callback** (after S3 event notification processes):
    ```
    POST /api/files/{upload_id}/complete  (internal, called by the processing pipeline)
    ```

    **File retrieval API**:
    ```
    GET /api/files/{file_id}
    Response: { "id": "file_abc", "original_name": "invoice.pdf", "content_type": "application/pdf", "size": 524288, "status": "completed", "download_url": "https://s3...presigned-url", "variants": { "thumb-128": "https://cdn.../thumb.jpg" }, "created_at": "..." }
    ```

    **File listing API**:
    ```
    GET /api/files?entity_type=order&entity_id=ord_123&category=document
    Response: { "data": [...], "pagination": { "next_cursor": "..." } }
    ```

    **File deletion API**:
    ```
    DELETE /api/files/{file_id}
    Response: 204 No Content (soft delete, actual S3 deletion happens async)
    ```

    **Multipart upload API** (for large files):
    ```
    POST /api/files/multipart/initiate
    Request: { "filename": "video.mp4", "content_type": "video/mp4", "size": 5368709120, "part_count": 50 }
    Response: { "upload_id": "upl_abc", "parts": [{ "part_number": 1, "upload_url": "https://s3..." }, ...] }

    POST /api/files/multipart/{upload_id}/complete
    Request: { "parts": [{ "part_number": 1, "etag": "..." }, ...] }
    Response: { "file_id": "file_abc", "status": "processing" }
    ```

    **API design rules**:
    - Never return raw S3 keys in API responses. Return presigned URLs or CDN URLs.
    - Include the file's `status` in responses so clients can show processing state.
    - Include variant URLs for each available variant so clients can select the appropriate size/format.
    - Rate-limit upload URL generation to prevent abuse (e.g., max 100 upload URLs per user per hour).
