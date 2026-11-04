# Phase 1: File Storage Requirements Discovery

1. **Identify the file storage domain.** Before any design, establish what files the system manages and why. If the user has not clearly stated this, ask: "What types of files does this system handle, who uploads and downloads them, and what happens to the files after they are stored?" Do not design file storage without understanding the domain.

   Establish the following:
   - **File types**: What categories of files does the system handle?
     - **User-generated content**: Profile photos, avatars, cover images, uploaded documents, form attachments.
     - **Media assets**: Product images, marketing photos, videos, audio files, podcast episodes.
     - **Documents**: PDFs, spreadsheets, presentations, contracts, legal documents, invoices.
     - **Application artifacts**: Generated reports, exported data (CSV, Excel), system-generated PDFs, receipts.
     - **Technical assets**: Log files, backup archives, database dumps, build artifacts, deployment packages.
     - **Compliance records**: Audit documents, signed agreements, identity verification documents, medical records.
   - **File formats**: Specific formats expected per category (JPEG, PNG, WebP, PDF, MP4, CSV, ZIP, etc.). Are there format restrictions? Must certain formats be validated or converted?
   - **File sizes**: Average and maximum file size per category. Small (< 1MB: avatars, thumbnails), medium (1-100MB: documents, high-res images), large (100MB-10GB: videos, archives), very large (> 10GB: raw media, database backups).

2. **Catalog the file access patterns.** For each file category, document how files are accessed:

   **Upload patterns**:
   - **Upload frequency**: Files per second/minute/hour at current and projected peak. (10 uploads/minute for a SaaS app vs. 10,000 uploads/second for a media platform.)
   - **Upload source**: Web browser, mobile app, API integration, server-side process, batch import, or streaming ingest.
   - **Upload size distribution**: What percentage of uploads are small (< 5MB), medium (5-100MB), large (> 100MB)?
   - **Upload concurrency**: How many simultaneous uploads must be supported?
   - **Upload reliability**: Must uploads be resumable? (Critical for large files over unreliable networks, especially mobile.)
   - **Upload validation**: What validation is required before or immediately after upload? (File type, file size, dimensions for images, virus scanning, content moderation.)

   **Download/access patterns**:
   - **Read frequency**: How often are files accessed after upload? Once (archive), occasionally (documents), frequently (product images), or very frequently (avatars, logos)?
   - **Read-to-write ratio**: Is this read-heavy (1000:1 — media serving), write-heavy (1:1 — backup storage), or balanced?
   - **Access recency**: Are recently uploaded files accessed most frequently (social media), or is access evenly distributed across file age (document management)?
   - **Geographic distribution**: Are consumers located in one region or globally distributed? (Affects CDN and replication decisions.)
   - **Access latency requirements**: How fast must files be served? < 50ms (product images on e-commerce pages), < 500ms (document downloads), < 5s (large file downloads starting).
   - **Bandwidth requirements**: Peak download throughput in MB/s or GB/s.
   - **Streaming requirements**: Must video or audio be streamed (adaptive bitrate: HLS/DASH), or downloaded completely before playback?

   **Transformation patterns**:
   - Are files served in their original format, or must they be transformed on upload or on request? (Image resizing, format conversion, thumbnail generation, video transcoding, document preview generation.)
   - How many variants are needed per file? (Product image: original + thumbnail + medium + large + WebP variants = 5 variants.)

   **Lifecycle patterns**:
   - How long must files be retained? Days (temporary uploads), months (user content), years (legal documents), indefinitely (archival)?
   - Do files become less frequently accessed over time? (Hot → warm → cold → archive transition.)
   - When and how are files deleted? User-initiated, admin-initiated, automated by retention policy, or never?

3. **Identify constraints and requirements.**
   - **Compliance**: GDPR (right to erasure — must delete files on request), HIPAA (PHI in files must be encrypted, access-audited), PCI-DSS (card images must be protected), data residency (files must be stored in specific geographic regions), legal holds (files must not be deleted during litigation).
   - **Security**: Who can upload files? Who can download files? Are there per-file or per-folder access controls? Are files public (anyone with the URL) or private (requires authentication)?
   - **Budget**: Storage costs, transfer costs, processing costs. Is cost optimization a priority?
   - **Existing infrastructure**: Cloud provider, existing storage services, CDN, existing file handling code.
   - **Team expertise**: Experience with object storage, CDN configuration, media processing pipelines.

4. **Produce the file storage catalog.** Summarize all file categories and their requirements:

   | Category | Formats | Avg Size | Max Size | Upload Rate | Read Frequency | Retention | Access Control | Processing |
   |---|---|---|---|---|---|---|---|---|
   | Profile photos | JPEG, PNG | 500KB | 10MB | 50/min | Very high | Account lifetime | Public (URL) | Resize: 64, 128, 256, 512px |
   | Product images | JPEG, PNG, WebP | 2MB | 20MB | 200/day | Very high | Product lifetime | Public (CDN) | Resize + WebP conversion |
   | Invoices (PDF) | PDF | 200KB | 5MB | 100/day | Low | 7 years | Private (authenticated) | None |
   | User uploads | Any | 5MB | 100MB | 500/day | Medium | 1 year after last access | Private (owner + admins) | Virus scan, type validation |
   | Video content | MP4 | 500MB | 5GB | 20/day | High | Content lifetime | Authenticated | Transcode HLS |
   | Database backups | SQL.gz | 2GB | 50GB | 4/day | Rare | 90 days | Internal only | None |

   This catalog is the foundation for all subsequent decisions.
