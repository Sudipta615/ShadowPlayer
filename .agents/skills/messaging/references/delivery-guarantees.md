# Phase 6: Delivery Guarantees and Idempotency

13. **Design at-least-once delivery.** At-least-once delivery means the system guarantees every message will be delivered, but a message may be delivered more than once. This is the standard for most business messaging. Implement it correctly on both producer and consumer sides:

    **Producer-side at-least-once**:
    - **Kafka**: Set `acks=all` (wait for all in-sync replicas to acknowledge), `retries=MAX_INT` (retry indefinitely on transient failures), `enable.idempotence=true` (prevent duplicate messages from producer retries). With idempotent producer enabled, Kafka deduplicates messages from the same producer session.
    - **RabbitMQ**: Use publisher confirms. After publishing, wait for the broker's `ack`. If `nack` or timeout, retry. Use mandatory flag if the message must be routed to at least one queue (broker returns unroutable messages to the producer).
    - **SQS**: SQS is inherently durable — once `SendMessage` returns successfully, the message is stored redundantly. Retry on transient HTTP errors.

    **Consumer-side at-least-once**:
    - **Process, then acknowledge**: The consumer must fully process the message (including any database writes, API calls, or side effects) before acknowledging it. If the consumer crashes after processing but before acknowledging, the message will be redelivered — which is why the consumer must be idempotent.
    - **Never acknowledge before processing**: If you acknowledge first and then crash, the message is lost (the broker thinks it was processed).
    - **Kafka**: Commit the offset only after successful processing. Use manual offset commit (`enable.auto.commit=false`) — auto-commit acknowledges messages on a timer regardless of processing status. Commit after each message or after each batch, depending on throughput requirements.
    - **RabbitMQ**: Use manual acknowledgment (`autoAck=false`). Send `basic.ack` only after successful processing. Send `basic.nack` or `basic.reject` with `requeue=true` for transient failures (message returns to queue) or `requeue=false` for permanent failures (message goes to DLQ).
    - **SQS**: Messages become invisible for the visibility timeout period after being received. After successful processing, explicitly delete the message. If not deleted before the visibility timeout expires, the message becomes visible again and is redelivered. Set visibility timeout longer than the maximum expected processing time.

14. **Design idempotent consumers.** Since at-least-once delivery means messages may arrive more than once, every consumer must produce the same result whether it processes a message once or ten times:

    **Idempotency strategies**:

    **Strategy 1: Idempotency key with deduplication store (recommended)**:
    - Use the `message_id` from the message envelope as the idempotency key.
    - Before processing, check if the `message_id` has already been processed:
      ```
      if database.exists(processed_messages, message_id):
          skip processing, acknowledge message
      else:
          process message
          insert message_id into processed_messages table
          acknowledge message
      ```
    - **Critical: The processing and the idempotency record insertion must be in the same database transaction.** If they are separate, a crash between processing and recording can cause duplicate processing.
    - Set a TTL on the deduplication store (e.g., 7 days — longer than the maximum possible redelivery window) to prevent unbounded growth.
    - Storage options: Database table (`processed_message_ids`), Redis SET with TTL, or application-specific deduplication.

    **Strategy 2: Natural idempotency (upsert/PUT semantics)**:
    - Design the processing operation to be naturally idempotent. If the consumer writes to a database, use `INSERT ... ON CONFLICT DO UPDATE` (upsert) keyed on the entity's natural identifier.
    - Example: "Set order status to 'shipped'" is naturally idempotent — setting it twice has the same effect as setting it once. "Increment order count by 1" is NOT idempotent — incrementing twice gives the wrong result.
    - Prefer upsert-style operations over insert-or-increment operations wherever possible.

    **Strategy 3: Conditional processing with version/sequence**:
    - Include a version number or sequence number in the message. The consumer only processes the message if the entity's current version matches the expected version:
      ```
      UPDATE orders SET status = 'shipped', version = 5
      WHERE id = 'ord_123' AND version = 4
      ```
    - If the version does not match (message already processed or a newer update arrived first), skip processing. This handles both deduplication and out-of-order delivery.

    For each consumer in the system, define the idempotency strategy. State it in the flow catalog: "Consumer: inventory-service for F-001. Idempotency: deduplication store using message_id in `processed_events` table, same transaction as inventory update."

15. **Design exactly-once processing where required.** True exactly-once in distributed systems is achieved by combining at-least-once delivery with idempotent processing, wrapped in a transactional boundary:

    **Kafka transactions (for Kafka-to-Kafka processing)**:
    - Kafka supports transactional producers and consumers: consume a message, process it, produce the output message(s), and commit the consumer offset — all atomically. If any step fails, the entire transaction is rolled back.
    - Enable with: `transactional.id` on the producer, `isolation.level=read_committed` on downstream consumers.
    - Use for: stream processing (consume from topic A, transform, produce to topic B) where you need exactly-once semantics within the Kafka ecosystem.
    - This does NOT provide exactly-once for external side effects (database writes, API calls). For those, use idempotent processing (step 14).

    **Transactional outbox (for database-to-message-broker)**:
    - When the application writes to a database AND publishes a message, both must succeed or both must fail. Dual-write (write to DB, then publish message) is unreliable — if the publish fails after the DB commit, the message is lost; if the publish succeeds but the DB commit fails, the message is orphaned.
    - **Outbox pattern**: Write both the business data and the outgoing message to the database in the same transaction. A separate process (outbox poller or CDC connector) reads the outbox table and publishes messages to the broker.
      ```sql
      BEGIN;
      INSERT INTO orders (id, status, ...) VALUES ('ord_123', 'placed', ...);
      INSERT INTO outbox (id, aggregate_type, aggregate_id, event_type, payload, created_at)
        VALUES ('msg_abc', 'order', 'ord_123', 'order.placed', '{"order_id": "ord_123", ...}', now());
      COMMIT;
      ```
    - The outbox poller reads unprocessed outbox entries, publishes them to the broker, and marks them as published. If the poller crashes, it retries on restart (at-least-once publishing — consumers must be idempotent).
    - **CDC-based outbox**: Use Debezium to capture changes from the outbox table's transaction log and publish to Kafka. More reliable than polling (captures changes in real-time, no polling interval delay, no missed changes).
    - This is the recommended pattern for reliable event publishing from a database. Use it whenever a service needs to update its database AND publish an event atomically.