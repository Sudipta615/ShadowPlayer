# Phase 6: File Metadata Management

16. **Design the file metadata schema.** File metadata must be stored in the application database — not only in object storage metadata (S3 tags/metadata have limited queryability and are not suitable for application-level queries):

    **File metadata table** (database schema):
    ```sql
    CREATE TABLE files (
        id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        tenant_id       UUID NOT NULL,
        owner_id        UUID NOT NULL REFERENCES users(id),
        storage_bucket  VARCHAR(255) NOT NULL,
        storage_key     VARCHAR(1024) NOT NULL,
        original_name   VARCHAR(255) NOT NULL,
        content_type    VARCHAR(127) NOT NULL,
        size_bytes       BIGINT NOT NULL,
        checksum_sha256 VARCHAR(64),
        status          VARCHAR(20) NOT NULL DEFAULT 'pending',  -- pending, processing, completed, rejected, deleted
        category        VARCHAR(50) NOT NULL,  -- avatar, document, product_image, backup
        visibility      VARCHAR(20) NOT NULL DEFAULT 'private',  -- public, private, internal
        upload_source   VARCHAR(20),  -- web, mobile, api, system
        metadata_json   JSONB,  -- flexible metadata: dimensions, duration, page count, etc.
        virus_scan_status VARCHAR(20) DEFAULT 'pending',  -- pending, clean, infected, error
        virus_scan_at    TIMESTAMPTZ,
        created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
        updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
        deleted_at      TIMESTAMPTZ,  -- soft delete
        UNIQUE (storage_bucket, storage_key)
    );

    CREATE INDEX idx_files_owner ON files(owner_id, category, created_at DESC);
    CREATE INDEX idx_files_tenant ON files(tenant_id, category, status);
    CREATE INDEX idx_files_status ON files(status) WHERE status = 'pending';
    ```

    **File variants table** (for transformed versions):
    ```sql
    CREATE TABLE file_variants (
        id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        file_id         UUID NOT NULL REFERENCES files(id),
        variant_type    VARCHAR(50) NOT NULL,  -- original, thumb-128, webp-800, hls-720p
        storage_bucket  VARCHAR(255) NOT NULL,
        storage_key     VARCHAR(1024) NOT NULL,
        content_type    VARCHAR(127) NOT NULL,
        size_bytes       BIGINT NOT NULL,
        width           INTEGER,  -- for images/video
        height          INTEGER,
        created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
        UNIQUE (file_id, variant_type)
    );

    CREATE INDEX idx_file_variants_file ON file_variants(file_id);
    ```

    **Entity-file association table** (linking files to application entities):
    ```sql
    CREATE TABLE entity_files (
        id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        entity_type     VARCHAR(50) NOT NULL,  -- product, order, user_profile, ticket
        entity_id       UUID NOT NULL,
        file_id         UUID NOT NULL REFERENCES files(id),
        purpose         VARCHAR(50) NOT NULL,  -- primary_image, attachment, receipt, avatar
        sort_order      INTEGER DEFAULT 0,
        created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
        UNIQUE (entity_type, entity_id, file_id, purpose)
    );

    CREATE INDEX idx_entity_files_entity ON entity_files(entity_type, entity_id, purpose);
    ```

    **Design principles**:
    - **Never store file bytes in the database.** Store only metadata and the storage key that references the file in object storage.
    - **Never construct storage URLs from the key at query time.** Generate presigned URLs or CDN URLs at the API layer, not in SQL.
    - **Track file status**: Pending → processing → completed → deleted. Only files with `status = 'completed'` should be served to users.
    - **Track file-entity associations separately**: A file can be associated with multiple entities (same product image used in multiple categories). An entity can have multiple files (product with 5 images).
    - **Soft delete**: When a user deletes a file, set `deleted_at`. The actual object storage deletion happens asynchronously (a background job deletes the S3 object after a grace period, or lifecycle rules handle it).
