### Phase 5: Physical Schema Design (Non-Relational Databases)

17. **Design document schemas (MongoDB, DynamoDB, Firestore).** For document databases, the schema design is driven entirely by access patterns:
    - **Embed vs. Reference decision** for each relationship:
      - **Embed** (denormalize into the parent document) when: the child data is always read with the parent, the child data is owned by the parent and has no independent lifecycle, the cardinality is bounded and small (1:few), and the child data doesn't change independently of the parent.
      - **Reference** (store a foreign ID and fetch separately) when: the related data is accessed independently, the cardinality is unbounded (1:many with many > 100), the related data changes frequently and independently, or the related data is shared across multiple parents.
    - **Design for the primary query, not for normalization.** In document databases, you optimize the write path and document structure for the most common read pattern. If you need to read customer + orders + line items in one call, embed or structure accordingly.
    - **Avoid unbounded arrays in documents.** A document that grows indefinitely (e.g., an array of all orders embedded in a customer document) will cause performance degradation and eventually hit document size limits (16MB in MongoDB). Use a separate collection with a reference when the array can grow unboundedly.

18. **Design DynamoDB table and key structures (if DynamoDB is selected).** DynamoDB requires access-pattern-first design:
    - **Define the partition key (PK)** based on the highest-cardinality, most-queried attribute. The partition key must distribute data evenly — avoid hot partitions.
    - **Define the sort key (SK)** to enable range queries within a partition. Use composite sort keys for hierarchical access: `SK = ORDER#2024-01-15#ord_123`.
    - **Design single-table or multi-table**: Single-table design (storing multiple entity types in one table) is recommended when: entities are frequently accessed together, and the access patterns are well-defined. Use multi-table when entities are unrelated and accessed by different services.
    - **Design Global Secondary Indexes (GSIs)** for each access pattern that cannot be satisfied by the base table's key structure. Each GSI has its own partition and sort key. Minimize GSI count — each GSI duplicates data and consumes additional write capacity.
    - **Design the item structure**: Overload PK/SK attributes with prefixed entity types (e.g., `PK: CUSTOMER#cust_123`, `SK: ORDER#2024-01-15`). Document the key schema exhaustively — DynamoDB single-table design is unreadable without documentation.

19. **Design wide-column schemas (Cassandra/ScyllaDB, if selected).** Cassandra requires one-table-per-query design:
    - **Define the partition key** to ensure even data distribution and to satisfy the query's equality predicates. A partition should not exceed 100MB.
    - **Define clustering columns** to satisfy the query's range predicates and sort order within a partition.
    - **Create a separate table for each distinct query pattern.** Duplicate data across tables — this is by design in Cassandra, not a mistake. Data is written redundantly to serve each query efficiently.
    - **Design the compaction strategy** based on the workload: Size-Tiered (write-heavy, default), Leveled (read-heavy, space-efficient), Time-Window (time-series data with TTL).