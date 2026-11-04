### Phase 2: Database Technology Selection

5. **Select the primary database technology based on the access pattern catalog.** Match the technology to the dominant access patterns — not to trends, not to familiarity, and not to a single feature. Evaluate each candidate against the full access pattern catalog from step 2.

   **Relational databases (PostgreSQL, MySQL, SQL Server)**:
   - **Select when**: Data has well-defined structure and relationships. Access patterns include complex joins, multi-table transactions, aggregations, and ad-hoc queries. ACID transactions are required for core operations. The data model is unlikely to change radically. Most access patterns involve structured queries with known predicates.
   - **PostgreSQL** (default recommendation for relational): Superior feature set — JSONB for semi-structured data within a relational model, partial indexes, expression indexes, CTEs, window functions, full-text search (adequate for moderate search needs), excellent extension ecosystem (PostGIS, pg_trgm, timescaledb). Choose PostgreSQL unless there is a specific reason not to.
   - **MySQL/Aurora MySQL**: When the team has deep MySQL expertise, when Aurora's architecture provides needed read scaling, or when specific MySQL ecosystem tools are required. MySQL 8.0+ has closed many feature gaps with PostgreSQL.
   - **SQL Server**: When the organization is Microsoft-ecosystem committed, or when specific SQL Server features (SSRS, SSIS, in-memory OLTP) are required.
   - **Managed service recommendation**: Amazon RDS/Aurora, Google Cloud SQL/AlloyDB, Azure Database for PostgreSQL/SQL. Always prefer managed unless regulatory or latency requirements mandate self-managed.

   **Document databases (MongoDB, Amazon DocumentDB, Firestore, Cosmos DB)**:
   - **Select when**: The data model is document-oriented — self-contained objects that are read and written as a unit. Access patterns are primarily key-based lookups or simple queries on top-level fields. The schema evolves frequently and different records may have different structures. Relationships between documents are minimal — if you frequently need to join across collections, a document database is the wrong choice.
   - **MongoDB**: When you need flexible schema, rich query capabilities within documents, aggregation pipeline, and the operational maturity of Atlas (managed).
   - **Do NOT select document databases because**: "We might change the schema later" (relational schema migrations are a solved problem), "JSON is easier" (PostgreSQL JSONB gives you JSON within a relational model), or "NoSQL scales better" (this is not inherently true and depends on access patterns).

   **Key-value stores (Redis, Memcached, DynamoDB, Valkey)**:
   - **Select when**: Access patterns are overwhelmingly key-based lookups and writes with known keys. Extremely low latency is required (sub-millisecond). The data model is simple — key → value with no complex querying.
   - **Redis / Valkey**: For caching, session storage, rate limiting, leaderboards, real-time counters, distributed locks, and pub/sub. Use as a caching layer or ephemeral data store, not as a primary database for durable data (unless using Redis with AOF persistence and accepting its operational characteristics).
   - **DynamoDB**: When you need a fully managed, serverless key-value/document store with predictable single-digit-millisecond latency at any scale, and your access patterns are well-defined at design time. DynamoDB requires you to design your data model around access patterns upfront — retrofitting new access patterns is expensive. Select DynamoDB when access patterns are stable, scale is high, and operational simplicity is paramount.

   **Wide-column stores (Apache Cassandra, ScyllaDB, Google Bigtable)**:
   - **Select when**: Write throughput is extremely high (tens of thousands to millions of writes per second). Data is append-heavy or time-series in nature. Queries are limited to partition key lookups and range scans within a partition. Multi-region, active-active replication is required natively. Tolerance for eventual consistency exists.
   - **Do NOT select when**: You need complex queries, joins, aggregations, or secondary indexes on arbitrary fields. Cassandra's query model is restrictive by design — every query must be satisfied by the table's partition and clustering key structure.

   **Search engines (Elasticsearch/OpenSearch)**:
   - **Select when**: Full-text search, fuzzy matching, faceted search, log analytics, or complex filtering and aggregation over semi-structured data are primary access patterns.
   - **Never use as the sole primary data store**: Search engines can lose data during failures and are not ACID-compliant. Always maintain a source-of-truth data store and feed the search engine via CDC, events, or dual-write with reconciliation.
   - **Consider PostgreSQL full-text search first**: For moderate search needs (< 10M documents, simple text matching), PostgreSQL's built-in full-text search with GIN indexes may be sufficient and eliminates the operational overhead of a separate search cluster.

   **Graph databases (Neo4j, Amazon Neptune, Memgraph)**:
   - **Select when**: The core query patterns involve multi-hop traversals across complex relationships — social networks (friends of friends), permission hierarchies (does user X have access to resource Y through group membership?), fraud detection (find circular transaction patterns), recommendation engines (users who bought X also bought Y via shared attributes).
   - **Do NOT select because**: "Our data has relationships." All relational databases handle relationships. Graph databases are justified only when the query pattern is fundamentally about traversing relationships of variable or unknown depth.

   **Time-series databases (TimescaleDB, InfluxDB, QuestDB, Amazon Timestream)**:
   - **Select when**: The primary data model is timestamped measurements — IoT sensor data, application metrics, financial tick data, user activity events. Queries are dominated by time-range scans, downsampling, and time-windowed aggregations. Data retention policies with automatic expiration are important.
   - **TimescaleDB**: Preferred when you want time-series capabilities as a PostgreSQL extension — you get time-series optimizations while retaining full SQL, joins with relational tables, and the PostgreSQL ecosystem.

   **Vector databases (Pinecone, Weaviate, pgvector, Milvus, Qdrant)**:
   - **Select when**: The system performs similarity search over high-dimensional embeddings — semantic search, recommendation systems, image/audio similarity, RAG (retrieval-augmented generation) for LLMs.
   - **pgvector**: Preferred when vector search is a secondary access pattern alongside relational data, and the embedding dataset is moderate (< 10M vectors). Keeps the operational footprint simple.
   - **Dedicated vector database**: When vector search is the primary access pattern, the dataset is large (100M+ vectors), and advanced features (filtering during ANN search, real-time index updates, sharding) are required.

   **Object storage (S3, GCS, Azure Blob Storage)**:
   - **Select for**: Files, media, backups, data lake raw storage, large binary objects. Never store large blobs (> 1MB) in a relational database — store a reference (URL/key) in the database and the blob in object storage.

6. **Justify the selection explicitly.** For every technology choice, state:
   - Which access patterns from the catalog (by number) this technology satisfies.
   - What the technology gives you (specific capabilities matched to specific requirements).
   - What the technology costs you (operational complexity, limitations, learning curve).
   - What alternatives were considered and why they were rejected for this specific system.
   - Under what conditions you would reconsider this choice.

7. **Design polyglot persistence when justified.** If no single database satisfies all access patterns, design a multi-database architecture:
   - Define which database handles which access patterns. Each access pattern must have exactly one primary data store.
   - Define the system of record for each entity — which database holds the authoritative version.
   - Design the synchronization mechanism between databases (CDC, events, dual-write with reconciliation). State the consistency model — how stale can secondary stores be?
   - State the operational cost of polyglot persistence: more systems to monitor, more failure modes, more expertise required. Only adopt when the access pattern mismatch genuinely cannot be solved by a single technology.