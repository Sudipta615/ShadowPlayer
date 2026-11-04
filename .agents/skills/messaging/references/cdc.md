# Phase 11: Change Data Capture (CDC) and Event Publishing

25. **Design reliable event publishing with CDC.** When a service needs to publish events based on database changes, the transactional outbox pattern (step 15) with CDC provides the most reliable mechanism:

    **Debezium + Kafka** (recommended for PostgreSQL/MySQL → Kafka):
    - Debezium reads the database's transaction log (PostgreSQL WAL, MySQL binlog) and publishes change events to Kafka topics.
    - **Architecture**: Database → Debezium connector (runs as a Kafka Connect source connector) → Kafka topics.
    - **Topic per table**: Debezium creates one Kafka topic per captured table by default: `dbserver.schema.table`.
    - **Outbox pattern with Debezium**: Instead of capturing all tables, configure Debezium to capture only the outbox table. Use the Debezium outbox event router to transform outbox rows into properly structured domain events.
    - **Advantages**: No application code changes needed for event publishing (Debezium reads the transaction log). Guaranteed capture of all changes (including direct SQL updates, migrations, admin tools). At-least-once delivery. Low latency (sub-second for most configurations).
    - **Operational considerations**: Debezium requires the database's transaction log to be available (WAL level = `logical` for PostgreSQL, `binlog_format = ROW` for MySQL). Monitor Debezium connector health, lag, and snapshot status. Plan for connector failure: Debezium tracks its position in the transaction log and resumes from the last committed position on restart.

    **Application-level outbox polling** (alternative when CDC is not available):
    - A scheduled process polls the outbox table every N seconds, publishes unpublished messages, and marks them as published.
    - **Advantages**: No CDC infrastructure needed. Works with any database.
    - **Disadvantages**: Polling interval introduces latency (messages are delayed by up to the polling interval). Polling at high frequency creates database load. Risk of missed messages if the poller crashes between reading and marking as published (mitigate with a `published_at IS NULL` query and marking after successful publish).
    - Recommendation: Use Debezium/CDC when Kafka is in the architecture. Use polling when the infrastructure does not support CDC or the message volume is low (< 100 msg/s).

26. **Design CDC event transformation.** Raw CDC events (row-level changes) are not domain events. Transform them:

    **Raw CDC event** (from Debezium):
    ```json
    {
      "op": "u",
      "before": { "id": "ord_123", "status": "confirmed", "updated_at": "..." },
      "after": { "id": "ord_123", "status": "shipped", "updated_at": "..." },
      "source": { "table": "orders", "lsn": "...", "ts_ms": "..." }
    }
    ```

    **Domain event** (what consumers should receive):
    ```json
    {
      "message_id": "msg_abc",
      "type": "order.shipped",
      "source": "order-service",
      "timestamp": "2024-01-16T14:00:00Z",
      "data": {
        "order_id": "ord_123",
        "shipped_at": "2024-01-16T14:00:00Z",
        "previous_status": "confirmed"
      }
    }
    ```

    **Transformation options**:
    - **Kafka Streams / ksqlDB**: Transform the CDC stream into domain event streams in real-time.
    - **Debezium SMTs (Single Message Transforms)**: Apply transformations within the Debezium connector (field renaming, filtering, routing).
    - **Dedicated transformer service**: A consumer that reads raw CDC events, transforms them into domain events, and publishes to domain event topics.
    - **Outbox table with pre-formatted events**: Write domain events (already in the correct format) to the outbox table in the application. Debezium publishes them as-is. This is the simplest and recommended approach — the application controls the event format.