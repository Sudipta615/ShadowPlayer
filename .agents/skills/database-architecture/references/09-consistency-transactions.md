### Phase 9: Consistency, Transactions, and Distributed Data Patterns

29. **Design the transaction strategy.** For each write operation in the access pattern catalog:
    - **Identify the transaction boundary**: What set of writes must succeed or fail atomically?
    - **Choose the isolation level** (for relational databases):
      - **Read Committed** (PostgreSQL default): Prevents dirty reads. Sufficient for most OLTP workloads. Recommended as default.
      - **Repeatable Read**: Prevents non-repeatable reads and phantom reads. Use for operations that read data, make a decision, and write based on that decision within the same transaction (e.g., "check balance, then debit").
      - **Serializable**: Full isolation, prevents all anomalies. Use only for critical financial operations where correctness is paramount and the performance cost is acceptable. Monitor for serialization failures and implement retry logic.
    - **Keep transactions short**: Long-running transactions hold locks, block other operations, and risk timeouts. If a business operation involves multiple steps with user interaction, break it into multiple transactions with compensating logic rather than holding a transaction open.
    - **Avoid distributed transactions (2PC) across databases**: They are brittle, slow, and operationally complex. Use saga patterns or event-driven eventual consistency instead.

30. **Design optimistic and pessimistic concurrency control.** For entities that are concurrently updated:
    - **Optimistic concurrency control (OCC)** (recommended as default): Add a `version` column (integer, incremented on each update) or use `updated_at` timestamp. On update, include `WHERE version = expected_version` — if no rows are affected, a concurrent update occurred, and the application retries or returns a conflict error. Use OCC when conflicts are rare.
    - **Pessimistic locking**: Use `SELECT ... FOR UPDATE` to lock rows during a transaction. Use when conflicts are frequent and the cost of retry is high. Be cautious of deadlocks — always acquire locks in a consistent order.
    - **Advisory locks**: For application-level coordination that doesn't need row-level locking (e.g., "only one instance should run this migration at a time"). Use PostgreSQL advisory locks or Redis-based distributed locks.

31. **Design eventual consistency patterns (for distributed systems).** When data must be synchronized across services or databases:
    - **Change Data Capture (CDC)**: Capture changes from the source database's transaction log and publish them as events. Tools: Debezium (Kafka Connect), AWS DMS, PostgreSQL logical replication. CDC is the most reliable way to synchronize databases without dual-write inconsistency.
    - **Transactional outbox pattern**: Write the event to an outbox table within the same database transaction as the data change. A separate process reads the outbox and publishes events to the message broker. Guarantees at-least-once event delivery without distributed transactions.
    - **Event-driven materialization**: Consume events from the source and build read-optimized projections in the consumer's database. Define the consistency lag tolerance and the mechanism for rebuilding projections from scratch if they become corrupt.
    - **Dual-write avoidance**: Never write to two separate data stores in a single application-level operation without a coordination mechanism (outbox, CDC, or saga). Dual writes without coordination will eventually diverge.

32. **Design CQRS (Command Query Responsibility Segregation) when justified.** CQRS separates the write model from the read model:
    - **When to use**: When read and write access patterns have fundamentally different shapes (e.g., writes are normalized and transactional; reads require complex joins, aggregations, or denormalized views), and the performance requirements cannot be met with a single model.
    - **Command side**: Normalized relational model optimized for writes and consistency.
    - **Query side**: Denormalized read models (materialized views, search indexes, pre-computed aggregations) optimized for specific read access patterns.
    - **Synchronization**: Events from the command side update the query side. Define the lag tolerance and the rebuild mechanism.
    - **Do NOT use CQRS as a default**: It doubles the complexity of the data architecture. Use it only when the read/write asymmetry is severe enough to justify the cost.

33. **Design event sourcing only when specifically justified.** Event sourcing stores state as a sequence of events rather than as current state:
    - **When to use**: When a complete, immutable audit trail of every state change is a core business requirement (financial ledgers, compliance-heavy systems). When temporal queries ("what was the state at time T?") are a primary access pattern. When the domain genuinely benefits from replaying events to derive state.
    - **When NOT to use**: As a general-purpose storage pattern. For simple CRUD systems. When "we might want an audit trail someday" — add an audit log table instead.
    - **If using event sourcing, design**:
      - The event store (append-only, immutable event log).
      - The event schema with versioning (events must be backward-compatible or transformable).
      - Snapshot strategy (snapshot current state every N events to avoid replaying thousands of events on read).
      - Projection/read model design and rebuild mechanism.
      - Compaction/archival strategy for old events.