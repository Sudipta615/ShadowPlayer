### Phase 1: Data Requirements Discovery and Access Pattern Analysis

1. **Identify the data domain and its purpose.** Before any modeling or technology selection, explicitly state what data this system manages and why. If the user has not clearly stated this, ask directly: "What are the core data entities this system manages, and what are the primary operations performed on them?" Do not proceed until the data domain is understood. State it in one to two sentences. Example: "This system manages e-commerce order data, including orders, line items, payments, and fulfillment records. The primary operations are order creation, status tracking, customer order history retrieval, and operational reporting."

2. **Catalog the access patterns exhaustively.** This is the single most important step in database architecture — technology selection, schema design, indexing, and partitioning all flow from access patterns. For each identified access pattern:
   - **Name the operation** descriptively (e.g., "Look up order by ID," "List orders for a customer sorted by date," "Search products by name and category," "Aggregate daily revenue by region").
   - **Classify the operation**: Read or Write. For reads: point lookup, range scan, full-text search, aggregation, or graph traversal. For writes: insert, update, upsert, delete, or append.
   - **Estimate frequency**: How often does this operation occur? Requests per second at current scale and at projected scale (1 year, 3 years).
   - **Estimate data volume**: How many rows/documents are read or written per operation? What is the expected result set size?
   - **Identify the lookup keys and filter conditions**: What fields are used to find the data? (e.g., "by customer_id and status, sorted by created_at descending").
   - **Determine latency requirements**: What is the acceptable response time? (e.g., "< 10ms for point lookups, < 100ms for customer order history, < 5s for monthly revenue report").
   - **Determine consistency requirements**: Must this operation see the latest write immediately (strong consistency), or is a delay of seconds/minutes acceptable (eventual consistency)?
   - **Classify criticality**: Is this on the critical user-facing path, a background process, or an analytical query?

   Produce a numbered access pattern catalog. This catalog is the foundation for every subsequent decision. Reference specific access pattern numbers when justifying design choices.

3. **Characterize the data profile.** Establish concrete facts about the data:
   - **Read-to-write ratio**: Is this system read-heavy (100:1), write-heavy (1:10), or balanced (1:1)? This fundamentally affects technology choice and optimization strategy.
   - **Data growth rate**: How many new records per day/month? What is the expected total data volume in 1 year, 3 years, 5 years?
   - **Record size**: Average and maximum size of individual records. Are there large text fields, binary data, or deeply nested structures?
   - **Data relationships**: Are relationships simple (foreign keys between a few tables) or complex (deep hierarchies, many-to-many, graph-like traversals)?
   - **Data mutability**: Is data mostly immutable after creation (append-only logs, events, transactions) or frequently updated (user profiles, inventory counts)?
   - **Data temperature**: What percentage of data is "hot" (accessed frequently), "warm" (accessed occasionally), or "cold" (rarely accessed, archival)?
   - **Temporal characteristics**: Is there a strong time-based component (time-series, event logs)? Do queries primarily filter by time ranges?
   - **Cardinality**: For key fields, what is the expected number of distinct values? (Affects index selectivity and partition distribution.)

4. **Identify constraints and requirements.** Establish:
   - **Regulatory and compliance requirements**: GDPR (right to deletion, data residency), HIPAA (audit logging, encryption), PCI-DSS (cardholder data isolation), SOC 2, or industry-specific regulations. These constrain technology choices, encryption requirements, and data lifecycle design.
   - **Team expertise**: What databases does the team have production experience operating? Introducing a new technology has a steep operational learning curve — factor this into every recommendation.
   - **Infrastructure constraints**: Cloud provider (AWS, GCP, Azure), existing managed services, VPC networking, region availability.
   - **Budget constraints**: Managed service costs, license costs for commercial databases, storage costs at projected volume.
   - **Existing systems**: What databases are already in production? Is there a mandate to consolidate or a preference to reuse existing infrastructure?
   - **Operational capacity**: Does the team have dedicated DBAs, or do application engineers manage the database? This affects the choice between self-managed and fully managed services.