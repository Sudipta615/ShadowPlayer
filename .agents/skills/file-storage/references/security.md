# Phase 7: File Security

17. **Design file encryption.** Protect stored files against unauthorized access to the underlying storage:

    **Server-Side Encryption (SSE)** — enable on all buckets by default:
    - **SSE-S3** (Amazon-managed keys): S3 manages the encryption keys. Zero configuration, automatic. Each object is encrypted with a unique key, and that key is encrypted with a root key that is rotated regularly. Sufficient for most use cases.
    - **SSE-KMS** (customer-managed keys via AWS KMS): You control the KMS key. Enables: key access policies (restrict which IAM roles can decrypt), key usage audit logging (CloudTrail logs every decryption), key rotation control, and the ability to revoke access by disabling the key. Required for compliance-sensitive files (HIPAA, PCI-DSS). Adds ~$0.03 per 10,000 encryption/decryption requests.
    - **SSE-C** (customer-provided keys): You provide the encryption key with each request. The key is not stored by S3. Use when you must maintain full key custody outside of AWS. Complex to manage — you are responsible for key storage, rotation, and availability.
    - **Recommendation**: Use SSE-S3 for non-sensitive files (public assets, cached transformations). Use SSE-KMS for sensitive files (user documents, compliance records, PII-containing files).

    **Client-Side Encryption (CSE)** — for maximum protection:
    - Files are encrypted in the application before uploading to S3. S3 stores ciphertext only. Decryption happens in the application after downloading.
    - **When to use**: When files must be protected even from cloud provider employees, from anyone with AWS console access, and from accidental S3 bucket policy misconfigurations. For extremely sensitive data (healthcare records, financial documents, encryption keys).
    - **Implementation**: Use envelope encryption. Generate a data encryption key (DEK), encrypt the file with AES-256-GCM, encrypt the DEK with a KMS key (KEK), store the encrypted DEK as object metadata alongside the encrypted file.
    - **Tradeoff**: Client-side encryption prevents all server-side processing (no S3 Select, no server-side transformation, no virus scanning without decryption). Processing must be done in the application layer after decryption.

18. **Design file access control.** Multiple layers of access control must work together:

    **Layer 1: Bucket policy and IAM (infrastructure-level)**:
    - Bucket policies define broad access rules: "This bucket is publicly readable," "Only this IAM role can write to this bucket."
    - IAM policies on service roles define what the application can do: "The order-service role can read/write to `s3://myapp-private-uploads/orders/*`."
    - **Principle of least privilege**: Each service's IAM role should have access only to the specific bucket prefixes it needs. The user-service can access `users/*`, the order-service can access `orders/*`, but neither can access the other's prefixes.

    **Layer 2: Presigned URLs (request-level)**:
    - Presigned URLs are the primary mechanism for granting temporary, scoped access to specific files.
    - The backend generates presigned URLs only after verifying the requesting user's authorization: "Is user X allowed to access file Y?" This authorization check happens in the application's authorization logic (RBAC, ABAC, resource ownership check).
    - Presigned URLs inherit the permissions of the IAM role that generated them — ensure the generating role has the minimum necessary S3 permissions.

    **Layer 3: Application-level authorization (business logic)**:
    - Before generating a presigned URL or serving a file, check: Does this user own this file? Is this file associated with an entity the user has access to? Is the user's role allowed to access this file category?
    - **Never rely on presigned URL obscurity as the sole access control.** Presigned URLs can be shared, bookmarked, and leaked. Use short expiry, and audit access patterns.

    **Public file security**:
    - For publicly accessible files (product images, public avatars), security is about preventing abuse, not restricting access:
      - Serve through CDN to absorb traffic and prevent direct S3 access costs.
      - Rate-limit requests to prevent hotlinking or scraping.
      - Use a separate domain for user-uploaded public content (e.g., `user-content.example-cdn.com`) to prevent cookie theft via XSS in uploaded HTML/SVG files.
      - Set `Content-Security-Policy` headers on the CDN to restrict execution of scripts in served files.

19. **Design CORS configuration for direct uploads.** When clients upload directly to S3 via presigned URLs from a web browser, S3 must accept cross-origin requests:

    **S3 CORS configuration**:
    ```json
    [
        {
            "AllowedHeaders": ["*"],
            "AllowedMethods": ["PUT", "POST"],
            "AllowedOrigins": ["https://app.example.com"],
            "ExposeHeaders": ["ETag", "x-amz-request-id"],
            "MaxAgeSeconds": 3600
        }
    ]
    ```
    - **AllowedOrigins**: Specify exact origins. Never use `"*"` in production — it allows any website to upload to your bucket.
    - **AllowedMethods**: Only the methods needed (PUT for presigned URL upload, POST for multipart).
    - **AllowedHeaders**: `"*"` is acceptable here — it allows the client to send any headers needed for the upload (Content-Type, Content-Length, x-amz-* headers).
    - **ExposeHeaders**: Include `ETag` (needed for multipart upload completion) and any custom headers the client needs to read from the response.
