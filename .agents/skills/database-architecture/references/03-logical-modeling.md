### Phase 3: Logical Data Modeling

8. **Build the logical data model.** Before any physical schema design, model the data conceptually:
   - **Identify all entities** in the domain. For each entity:
     - Name it clearly and specifically.
     - State its definition in one sentence.
     - List its attributes with data types, nullability, and business constraints.
     - Classify it: is this an independent entity (e.g., Customer), a dependent entity (e.g., OrderLineItem, which cannot exist without an Order), or a reference/lookup entity (e.g., Country, Currency)?
   - **Identify all relationships** between entities:
     - State the relationship type: one-to-one, one-to-many, many-to-many.
     - State the cardinality with specificity: "One Customer has zero to many Orders. One Order belongs to exactly one Customer."
     - State the ownership/lifecycle dependency: "When an Order is deleted, its LineItems are deleted (cascade). When a Customer is deleted, their Orders are retained (no cascade, set customer reference to null or anonymized)."
   - **Identify entity lifecycle states.** For entities with state machines (Orders: created → confirmed → shipped → delivered → returned), define the valid states and valid transitions. This informs status field design and constraint enforcement.
   - Produce an entity-relationship description or diagram. This model is technology-agnostic — it represents the business domain, not the physical storage.

9. **Identify aggregate boundaries (for document and DDD-oriented models).** An aggregate is a cluster of entities that are always read and written together as a consistency boundary:
   - Define which entities belong to the same aggregate. Example: Order + LineItems + ShippingAddress form an aggregate; Customer is a separate aggregate.
   - The aggregate root (e.g., Order) is the only entity directly accessible from outside — LineItems are accessed through the Order.
   - Transactions should not span multiple aggregates. If a business operation touches multiple aggregates, use eventual consistency (events, sagas) rather than distributed transactions.
   - Aggregate boundaries directly inform document structure (in document databases) and table decomposition (in relational databases).

10. **Design for data integrity at the model level.** Before physical implementation, define:
    - **Uniqueness constraints**: Which fields or combinations must be unique? (e.g., email per tenant, SKU per merchant.)
    - **Referential integrity**: Which relationships must be enforced at the database level vs. application level? In relational databases, use foreign keys for critical relationships. In document databases, accept that referential integrity is the application's responsibility and design for it.
    - **Business rule constraints**: CHECK constraints, valid value ranges, conditional requirements (e.g., "if status = 'shipped', tracking_number must not be null").
    - **Temporal integrity**: If the model tracks time (created_at, updated_at, deleted_at), define the timestamp source (database server time via `now()` vs. application time) and the timezone standard (UTC always — never local time).