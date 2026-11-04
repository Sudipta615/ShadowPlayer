# Phase 4: File Upload Design

9. **Design the upload architecture.** How files are uploaded is one of the most critical file storage decisions. The wrong upload pattern causes: server memory exhaustion, timeout errors, poor user experience, and security vulnerabilities.

   **Upload pattern selection**:

   **Pattern 1: Presigned URL upload (client → object storage directly)** — recommended as default:
   - The client requests an upload URL from the backend API. The backend generates a presigned URL (S3 PutObject presigned URL) with a short expiry and returns it to the client. The client uploads the file directly to object storage using the presigned URL. No file data passes through the backend.
   - **Flow**:
     1. Client sends `POST /api/uploads` with file metadata (filename, content type, size).
     2. Backend validates: Is the user authorized to upload? Is the file type allowed? Is the file size within limits?
     3. Backend generates a presigned S3 PutObject URL with conditions (max file size, content type, key, expiry = 15-60 minutes).
     4. Backend creates a pending upload record in the database: `{ upload_id, user_id, storage_key, status: 'pending', created_at }`.
     5. Backend returns `{ upload_url, upload_id, storage_key, expires_at }` to the client.
     6. Client uploads the file directly to S3 using the presigned URL (HTTP PUT).
     7. S3 sends an event notification (S3 Event → SQS/Lambda/EventBridge) when the upload completes.
     8. Backend receives the notification, validates the uploaded file (size, type, virus scan), updates the upload record to `status: 'completed'`, and links the file to the relevant entity.
   - **Advantages**: No file data passes through the backend servers (eliminates backend memory/CPU/bandwidth bottleneck). Backend servers remain small and fast. Supports files of any size. Leverages S3's massive upload infrastructure. Client can show upload progress directly.
   - **Disadvantages**: Requires two API calls (request URL + upload). Client must handle presigned URL expiry. Validation happens after upload (not during). Requires S3 event notifications or polling for upload completion.
   - **Security**: The presigned URL is scoped to a specific key, content type, and maximum file size. It expires after the configured duration. It does not grant access to any other objects. Include `Content-Type` and `Content-Length-Range` conditions in the presigned URL to prevent type spoofing and oversized uploads.

   **Pattern 2: Presigned POST (S3 POST policy)** — for browser-based uploads with more control:
   - Similar to presigned URL but uses an S3 POST policy (form-based upload). Allows setting conditions directly in the policy: exact key prefix, content type, size range, custom metadata.
   - **Advantages over presigned PUT**: More granular conditions. Supports redirect on completion (useful for simple form-based uploads).
   - **When to use**: Browser-based uploads where you need server-enforced conditions without JavaScript (rare in modern SPAs).

   **Pattern 3: Proxy upload (client → backend → object storage)** — use only when necessary:
   - The client uploads the file to the backend API. The backend streams the file to object storage.
   - **Flow**: Client sends multipart/form-data POST to backend API → backend streams the body to S3 (without buffering the entire file in memory) → backend returns the file metadata.
   - **When to use**: When the backend must process the file before storing it (real-time validation, transformation, virus scanning that must happen before storage). When presigned URLs are not feasible (complex authorization that cannot be expressed in presigned URL conditions). When the file must be processed as part of a synchronous API response.
   - **Critical implementation rules**:
     - **Stream, do not buffer**: Never read the entire file into memory. Use streaming I/O to pipe the upload body directly to S3's multipart upload API. In Node.js: pipe the readable stream to the S3 upload. In Go: pass the request body `io.Reader` directly to the S3 uploader.
     - **Set request body size limits**: Configure the web server/reverse proxy to reject requests exceeding the maximum allowed file size before they consume server resources. Nginx: `client_max_body_size 100m`. Express: `app.use(express.json({ limit: '100mb' }))`.
     - **Set timeouts**: Upload requests take longer than normal API requests. Set a longer timeout for upload endpoints (e.g., 5 minutes) while keeping short timeouts on other endpoints.
   - **Disadvantages**: Backend becomes a bottleneck (file data passes through it). Consumes backend bandwidth, CPU, and memory. Limits scalability for large files. Adds latency.

   **Recommendation**: Use presigned URL upload (Pattern 1) as the default. Use proxy upload (Pattern 3) only when synchronous server-side processing before storage is a hard requirement.

10. **Design multipart upload for large files.** For files larger than 5-10MB, multipart upload is essential:

    **S3 multipart upload**:
    - Files are split into parts (minimum 5MB per part, except the last part). Each part is uploaded independently, potentially in parallel. After all parts are uploaded, the upload is completed (parts are assembled into the final object).
    - **Advantages**: Parallel part uploads (faster for large files). Resumability (if a part fails, only that part is retried, not the entire file). No single request timeout for the entire file. Required for files > 5GB (S3's single PUT maximum).
    - **For presigned URL uploads**: Generate presigned URLs for each part (using CreateMultipartUpload, UploadPart presigned URLs, CompleteMultipartUpload). The client manages the multi-part upload process.
    - **For proxy uploads**: Use the S3 SDK's high-level upload manager (e.g., `s3.Upload` in Go, `Upload` from `@aws-sdk/lib-storage` in Node.js) which handles multipart automatically.

    **Client-side chunked upload (application-level chunking)**:
    - For very large files (> 100MB) or unreliable networks (mobile), implement application-level chunked upload:
      1. Client requests an upload session: `POST /api/uploads/sessions` → returns `{ session_id, chunk_size, chunk_urls: [...] }`.
      2. Client splits the file into chunks and uploads each chunk to its presigned URL (or to the backend API).
      3. Client reports progress and handles individual chunk retries.
      4. After all chunks are uploaded, client calls `POST /api/uploads/sessions/{session_id}/complete`.
      5. Backend assembles the chunks (S3 CompleteMultipartUpload) and creates the final file record.
    - **Resumability**: Track uploaded chunks per session. If the upload is interrupted, the client can resume by uploading only the missing chunks. Store session state (which chunks are uploaded) in Redis or the database with a TTL (24-72 hours).
    - **Chunk size**: 5-10MB per chunk for most use cases. Larger chunks (50-100MB) for very fast networks. Smaller chunks (1-5MB) for mobile/unreliable networks.

11. **Design upload validation.** Validate every uploaded file — user uploads are untrusted input:

    **Pre-upload validation** (before the file is uploaded):
    - File type: Validate the file extension and declared content type against an allowlist. Reject disallowed types before generating the presigned URL.
    - File size: Validate the declared file size against the maximum for the file category. Include `Content-Length-Range` in presigned URL conditions.
    - User authorization: Verify the user is allowed to upload to this entity/context.
    - Quota: Check if the user/tenant has remaining storage quota.

    **Post-upload validation** (after the file is uploaded to object storage):
    - **Magic byte validation**: Read the first bytes of the file to verify the actual file type matches the declared type. Do not trust the file extension or Content-Type header — both are user-controlled and trivially spoofed. Use a library that detects file type from magic bytes (e.g., `file-type` in Node.js, `filetype` in Go, `python-magic` in Python).
    - **Image validation**: For image uploads, attempt to decode the image. If it fails, the file is not a valid image (may be a renamed executable or malicious file). Validate dimensions (reject images with extreme dimensions: 50,000×50,000 pixels could cause memory exhaustion during processing).
    - **Virus/malware scanning**: For user-uploaded files that will be shared with other users or downloaded by internal staff, scan with an antivirus engine. Options: ClamAV (open-source, self-hosted), AWS GuardDuty for S3, cloud-based scanning APIs (VirusTotal, Sophos). Scan before the file is made accessible to other users — keep files in a `pending` state until scanning completes.
    - **Content moderation** (for user-generated content): For images/videos that will be publicly visible, use content moderation APIs (AWS Rekognition, Google Cloud Vision, Azure Content Moderator) to detect inappropriate content. Flag for review or auto-reject based on policy.
    - **File size verification**: After upload, verify the actual file size matches the declared size (prevents manipulation of presigned URL conditions).

    **Validation pipeline**:
    1. File is uploaded to a staging key/prefix: `uploads/pending/{upload_id}/{filename}`.
    2. S3 event notification triggers a validation Lambda/worker.
    3. Validator checks: magic bytes, file size, virus scan, content moderation (if applicable).
    4. If valid: move (copy + delete) the file to the final key: `users/{user_id}/avatar/{hash}.jpg`. Update database record to `status: 'completed'`.
    5. If invalid: delete the file. Update database record to `status: 'rejected'` with reason. Notify the user.
    6. If validation times out: alert operations. The file remains in `pending` state. Clean up pending files older than 24 hours via lifecycle rule.
