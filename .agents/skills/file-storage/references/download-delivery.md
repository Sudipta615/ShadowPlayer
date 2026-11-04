# Phase 5: File Download and Delivery Design

12. **Design the download architecture.** How files are served to consumers directly affects user experience, security, and cost:

    **Private file access via presigned URLs** — recommended for private/authenticated files:
    - The client requests a download URL from the backend API. The backend verifies authorization (is this user allowed to access this file?), generates a presigned S3 GetObject URL with a short expiry (5-60 minutes), and returns it to the client. The client downloads the file directly from S3.
    - **Advantages**: No file data passes through the backend. S3 handles the bandwidth. Backend stays fast and lightweight.
    - **URL expiry**: Set expiry based on the use case:
      - Image display in a web page: 60 minutes (long enough for the page session). Include the presigned URL in the API response that returns the entity data.
      - Document download: 5-15 minutes (user clicks download, URL expires quickly).
      - Streaming video: 60-120 minutes (long enough for the viewing session).
    - **Security**: Presigned URLs are bearer tokens — anyone with the URL can access the file until it expires. For highly sensitive files, use very short expiry (5 minutes) and require the client to request a new URL for each access. Do not log presigned URLs in server access logs (they contain the signature).
    - **Caching presigned URLs**: Presigned URLs should generally not be cached because they have a bounded lifetime. If you cache API responses that contain presigned URLs, the cached URLs may have expired when the consumer tries to use them. Options: generate presigned URLs with a lifetime longer than the cache TTL, or exclude presigned URLs from caching.

    **Public file access via CDN** — for publicly accessible files:
    - Files that are publicly accessible (product images, marketing assets, public user avatars) should be served through a CDN for performance and cost optimization.
    - Configure the S3 bucket as the CDN's origin. The CDN caches files at edge locations globally.
    - **CDN URL pattern**: `https://cdn.example.com/products/prod_456/images/front-view.jpg`.
    - Set long `Cache-Control` headers for immutable content-addressed files: `Cache-Control: public, max-age=31536000, immutable`. If the image changes, use a new key (append a version or hash to the filename), not the same key.
    - For files that change (user avatar that can be re-uploaded): use a shorter TTL (`max-age=3600`) or include a version token in the URL: `cdn.example.com/users/usr_abc/avatar/v3/photo.jpg`.

    **Private file access via signed CDN URLs** — for authenticated files with CDN performance:
    - For private files that are accessed frequently enough to benefit from CDN caching (authenticated video streaming, premium content):
      - Use CloudFront signed URLs or signed cookies. The backend generates a signed URL using a CloudFront key pair (not an S3 presigned URL). CloudFront validates the signature at the edge and serves from cache or fetches from the S3 origin.
      - **Advantages over S3 presigned URLs**: CDN caching (same file served to multiple authenticated users is cached at the edge). Lower latency (edge delivery). Lower S3 request costs (CDN absorbs repeated requests).
      - **Signed cookies**: For streaming scenarios where multiple requests are made for the same content (HLS segments), use signed cookies instead of signed URLs. The cookie covers a URL pattern (e.g., `https://cdn.example.com/videos/vid_123/*`), and all requests matching the pattern are authorized.

    **Direct S3 public hosting** — for simple static file serving:
    - S3 static website hosting allows serving files directly from S3 with a custom domain and index/error documents.
    - **When to use**: Static websites, single-page application hosting (HTML/CSS/JS), or simple file serving without a CDN.
    - **When NOT to use**: For dynamic content, API responses, or files that require authentication. For high-traffic sites, always add a CDN in front of S3 — direct S3 serving is slower and more expensive per request than CDN-cached serving.

13. **Design file serving optimization.**

    **Content-Type and Content-Disposition headers**:
    - Set the correct `Content-Type` when storing files in S3. If not set, S3 defaults to `application/octet-stream`, which causes browsers to download instead of displaying images.
    - For files that should be displayed inline (images, videos, PDFs in browser): `Content-Disposition: inline`.
    - For files that should be downloaded: `Content-Disposition: attachment; filename="original-filename.pdf"`. Include the original filename (stored in database metadata) so the user gets a meaningful filename.
    - Set these headers either as S3 object metadata at upload time, or override them in the presigned URL parameters (`ResponseContentDisposition`, `ResponseContentType`).

    **Range requests** (for large files and streaming):
    - S3 natively supports HTTP range requests (`Range: bytes=0-1023`). This enables:
      - Video/audio seeking without downloading the entire file.
      - Resumable downloads (client requests only the remaining bytes after interruption).
      - Parallel download (client downloads multiple ranges simultaneously and assembles them).
    - No additional configuration needed — S3 handles range requests automatically.

    **Transfer acceleration** (for geographically distant uploads):
    - S3 Transfer Acceleration routes uploads through CloudFront edge locations and AWS's backbone network, improving upload speed for clients far from the S3 bucket's region.
    - Enable per-bucket. Adds a small per-GB cost. Use when upload latency from distant geographic locations is a problem.
    - Alternative: Use a multi-region upload proxy that accepts uploads at the nearest region and transfers to the primary bucket.

14. **Design image optimization and responsive delivery.** For applications that serve many images (e-commerce, social media, content platforms):

    **Image transformation approaches**:

    **Approach 1: Transform on upload (pre-generation)** — recommended for predictable variant sets:
    - When a file is uploaded, generate all required variants (thumbnails, resized versions, WebP conversions) immediately or asynchronously.
    - Store each variant as a separate object: `products/prod_456/images/original/front.jpg`, `products/prod_456/images/thumb-128/front.jpg`, `products/prod_456/images/webp-800/front.webp`.
    - **Advantages**: Predictable storage cost. No processing latency on download. Variants are pre-cached by CDN. Simplest operational model.
    - **Disadvantages**: Storage cost for multiple variants per image. If variant requirements change, must reprocess all existing images. Wasted storage for variants that are never accessed.
    - **Implementation**: S3 event notification → processing Lambda/worker → generate variants → store variants → update database with variant URLs.

    **Approach 2: Transform on request (on-the-fly / dynamic)** — for flexible, unpredictable variant needs:
    - When a client requests an image, a transformation service generates the requested variant in real-time. The result is cached in the CDN and/or object storage.
    - URL pattern: `https://images.example.com/products/prod_456/front.jpg?w=800&h=600&format=webp&quality=80`.
    - **Implementation**: CDN (CloudFront/Cloudflare) → origin server (Lambda@Edge, Cloudflare Worker, or a dedicated image service like Imgproxy, Thumbor, Imaginary) → fetch original from S3 → transform → return → CDN caches.
    - **Advantages**: No pre-generation of variants. Supports unlimited dimension/format combinations. Clients request exactly what they need. Only variants that are actually requested are generated.
    - **Disadvantages**: First request for each variant incurs processing latency (100-500ms). Transformation service is a potential bottleneck. Must protect against abuse (attacker requests thousands of unique transformations to exhaust resources — use URL signing or dimension allowlists).
    - **CDN caching**: After the first request, the transformed image is cached at the CDN edge. Subsequent requests for the same transformation are served from cache.

    **Approach 3: Hybrid** — recommended for most applications:
    - Pre-generate a small set of standard variants on upload (thumbnail-128, medium-512, large-1024).
    - Use on-the-fly transformation for non-standard sizes requested dynamically.
    - Covers the common case with pre-generated variants and the edge cases with on-the-fly.

    **Image format strategy**:
    - Serve modern formats (WebP, AVIF) to clients that support them. Fall back to JPEG/PNG for older clients.
    - Implementation: The CDN or image service checks the `Accept` header (`Accept: image/webp, image/avif, */*`) and serves the optimal format. Or generate format-specific URLs in the API response based on the client's capabilities.
    - WebP: ~25-35% smaller than JPEG at equivalent quality. Supported by all modern browsers.
    - AVIF: ~50% smaller than JPEG. Newer, growing browser support. Higher encoding cost.

15. **Design video and audio delivery.** For media streaming:

    **Adaptive bitrate streaming (HLS/DASH)**:
    - Transcode source video into multiple bitrate variants (240p, 480p, 720p, 1080p) and segment each variant into small chunks (2-10 seconds).
    - Generate a manifest file (HLS: `.m3u8`, DASH: `.mpd`) that lists all variants and their segments.
    - The video player fetches the manifest, selects the appropriate bitrate based on network conditions, and downloads segments sequentially.
    - **Storage structure**:
      ```
      videos/vid_123/hls/manifest.m3u8
      videos/vid_123/hls/720p/segment-001.ts
      videos/vid_123/hls/720p/segment-002.ts
      videos/vid_123/hls/1080p/segment-001.ts
      ...
      ```
    - **Transcoding pipeline**: Upload original → S3 event → transcoding service (AWS MediaConvert, FFmpeg on EC2/ECS, Mux) → store transcoded segments in S3 → update database with manifest URL.
    - **Serve via CDN**: All video segments are served through the CDN. HLS/DASH segments are small and highly cacheable.
    - **Access control**: Use signed cookies (not signed URLs) for HLS — a single signed cookie covers all segment URLs matching a pattern.

    **Simple video download** (for non-streaming use cases):
    - Store the video file directly in S3. Serve via presigned URL or CDN.
    - S3 supports range requests, enabling seeking in downloaded video. However, without adaptive bitrate, users on slow connections will experience buffering.
