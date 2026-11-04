# Phase 1: Messaging Requirements Discovery

1. **Identify the communication requirement.** Before selecting any messaging technology or pattern, establish why asynchronous communication is needed. If the user says "we need a message queue," that is a solution, not a requirement. Ask and clarify until the communication requirement is explicit:

   - **What triggers the message?** A user action (order placed), a system event (file uploaded), a scheduled process (daily report), a state change (order status updated), or a data change (row inserted in database)?
   - **Who produces the message?** Which service, component, or system generates it?
   - **Who consumes the message?** One specific consumer (point-to-point), multiple known consumers (fan-out to specific services), or unknown/future consumers (publish for general consumption)?
   - **What action does the consumer take?** Send a notification, update a database, trigger a workflow, update a search index, generate a report, synchronize data to another system?
   - **Why must this be asynchronous?** Identify the specific reason:
     - **Decoupling**: The producer should not need to know about consumers or wait for them.
     - **Reliability**: The work must be done even if the consumer is temporarily unavailable.
     - **Throughput**: The producer generates work faster than any single consumer can process it.
     - **Latency isolation**: The producer must respond to the user quickly without waiting for slow downstream operations.
     - **Fan-out**: Multiple consumers need to react to the same event independently.
     - **Ordering**: Work must be processed in a specific order that synchronous parallel processing cannot guarantee.
   - **Is synchronous communication actually sufficient?** If the producer needs the result immediately, the consumer is always available, and the operation is fast (< 100ms), synchronous HTTP/gRPC may be simpler and more appropriate. Do not introduce messaging for operations that are naturally synchronous. The complexity cost of messaging (eventual consistency, error handling, monitoring) must be justified.

   State the communication requirement explicitly: "The order service needs to notify the inventory service, shipping service, and notification service when an order is placed. The order service should not wait for these downstream operations (they take 2-10 seconds combined). Each downstream service must process the event independently. If a downstream service is temporarily unavailable, the event must be retained and processed when the service recovers."

2. **Define the messaging requirements.** For each identified message flow, establish concrete specifications:

   **Delivery guarantee**:
   - **At-most-once**: The message may be lost, but will never be delivered more than once. Acceptable for: metrics, logs, non-critical notifications where occasional loss is tolerable.
   - **At-least-once** (recommended default): The message is guaranteed to be delivered at least once, but may be delivered more than once. The consumer must be idempotent. Acceptable for: most business operations, event processing, data synchronization.
   - **Exactly-once** (effectively: at-least-once delivery + idempotent processing): The message is delivered at least once, and the consumer's processing is designed to produce the same result regardless of how many times the message is delivered. This is the practical interpretation of "exactly-once" — true exactly-once delivery across distributed systems is extremely difficult and usually unnecessary if the consumer is idempotent.
   - State the chosen guarantee for each flow and why.

   **Ordering requirements**:
   - **No ordering required**: Messages can be processed in any order. Simplest, enables maximum parallelism.
   - **Per-entity ordering**: Messages for the same entity (same order, same customer, same account) must be processed in order, but messages for different entities can be processed in parallel. Most common requirement.
   - **Total ordering**: All messages must be processed in the exact order they were produced. Severely limits parallelism — only one consumer can process the stream. Rarely necessary. Only recommend if the business logic genuinely requires it.
   - State the ordering requirement for each flow and the ordering key (the field that defines "same entity").

   **Throughput and latency**:
   - **Message production rate**: Messages per second at current and projected peak load.
   - **Acceptable processing latency**: How quickly must a message be consumed after production? (< 1 second for near-real-time, < 1 minute for operational, < 1 hour for batch-like, hours/days for archival processing.)
   - **Message size**: Average and maximum message payload size. (Small: < 1KB, Medium: 1-100KB, Large: > 100KB.)
   - **Burst handling**: Can the system experience traffic spikes? (10x normal for 5 minutes, 100x for 30 seconds.) How should these be handled — buffered and processed gradually, or processed in real-time?

   **Retention and replay**:
   - **How long must messages be retained?** Only until consumed (queue semantics), or for a configurable retention period regardless of consumption (log/stream semantics)?
   - **Is replay required?** Must consumers be able to re-read historical messages? (For reprocessing after a bug fix, for new consumers that need to process historical events, for audit purposes.)
   - **Consumer independence**: Must each consumer track its own position in the message stream independently? (Yes for event streams, no for task queues.)

   **Reliability and durability**:
   - **What happens if a message is lost?** Is there a reconciliation mechanism? What is the business impact?
   - **What is the acceptable data loss window?** Zero (every message must be persisted before acknowledgment), or a small window (a few seconds of messages can be lost on infrastructure failure)?
   - **What is the recovery time objective?** If the messaging system is down, how long can the system operate without it?

3. **Identify all message flows.** Produce a comprehensive catalog of message flows in the system:

   | Flow ID | Producer | Consumer(s) | Event/Command | Delivery | Ordering | Rate (msg/s) | Latency Target | Retention |
   |---|---|---|---|---|---|---|---|---|
   | F-001 | order-service | inventory-service | OrderPlaced (event) | At-least-once | Per order_id | 500 peak | < 5s | 7 days |
   | F-002 | order-service | notification-service | OrderPlaced (event) | At-least-once | None | 500 peak | < 30s | 24 hours |
   | F-003 | order-service | analytics-service | OrderPlaced (event) | At-least-once | None | 500 peak | < 5 min | 30 days |
   | F-004 | payment-service | order-service | PaymentCompleted (event) | At-least-once | Per order_id | 200 peak | < 5s | 7 days |
   | F-005 | api-gateway | image-processor | ProcessImage (command) | At-least-once | None | 100 peak | < 60s | Until processed |
   | F-006 | user-service | search-indexer | UserUpdated (event) | At-least-once | Per user_id | 50 peak | < 30s | 24 hours |

   This catalog is the foundation for all subsequent decisions. Reference flow IDs when justifying design choices.
