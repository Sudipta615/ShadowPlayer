# Phase 4: Message Design and Schema

8. **Design the message structure.** Every message must have a well-defined structure with clear separation between metadata (envelope) and business data (payload):

   **Message envelope** (metadata — present on every message):
   ```json
   {
     "message_id": "msg_a1b2c3d4-e5f6-7890-abcd-ef1234567890",
     "type": "order.placed",
     "source": "order-service",
     "timestamp": "2024-01-15T10:30:00.123Z",
     "version": "1.2",
     "correlation_id": "req_xyz789",
     "causation_id": "msg_previous_id",
     "data": { ... }
   }
   ```

   - **`message_id`** (mandatory): Globally unique identifier for this specific message instance. Used for deduplication and idempotency. Generate using UUIDv4 or UUIDv7 (time-ordered for log analysis). Never reuse message IDs.
   - **`type`** (mandatory): The message type, identifying what this message represents. Use a dotted, hierarchical, lowercase naming convention: `{domain}.{entity}.{action}` — `order.placed`, `payment.completed`, `user.profile.updated`, `inventory.reserved`. This enables routing, filtering, and consumer subscription by type or type prefix.
   - **`source`** (mandatory): The service or system that produced the message. Identifies the origin for debugging and routing: `order-service`, `payment-gateway-adapter`.
   - **`timestamp`** (mandatory): When the event occurred (not when the message was published, if different). ISO 8601 with timezone (always UTC): `2024-01-15T10:30:00.123Z`. Include millisecond precision.
   - **`version`** (mandatory): Schema version of the message payload. Enables consumers to handle different versions of the same message type. Use semantic versioning: `1.0`, `1.1`, `2.0`.
   - **`correlation_id`** (mandatory for traceability): Identifier linking this message to the original request that initiated the workflow. Propagated across all messages in the same workflow. Enables end-to-end tracing of asynchronous flows.
   - **`causation_id`** (recommended): The `message_id` of the message that directly caused this message to be produced. Enables causal chain reconstruction: "message C was caused by message B, which was caused by message A."
   - **`data`** (mandatory): The message payload. Business-specific content. Structure depends on the message type.

   **Optional envelope fields** (include when relevant):
   - **`tenant_id`**: For multi-tenant systems, identifies the tenant. Enables tenant-scoped processing and routing.
   - **`trace_id`**: OpenTelemetry trace ID for distributed tracing integration.
   - **`partition_key`**: Explicit partitioning key (if different from an implicit key derived from the data). Used by the messaging infrastructure for ordering.
   - **`scheduled_at`**: For delayed messages, when the message should become visible to consumers.
   - **`expires_at`**: Message expiry. Consumer should discard the message if processing occurs after this time (prevents processing stale commands).
   - **`content_type`**: Serialization format of the data field: `application/json`, `application/protobuf`, `application/avro`.

9. **Design the message payload.** The payload must contain all information the consumer needs to process the message without calling back to the producer:

   **Event payloads** — what happened:
   - Include the relevant state at the time of the event: the entity's key attributes, the change that occurred, and any context needed to process the event.
   - **Event notification** (thin event): Contains minimal data — just the entity ID and event type. Consumer calls the source service to fetch full details. Simpler payload, but creates coupling (consumer depends on the source service's API availability).
     ```json
     { "order_id": "ord_123", "customer_id": "cust_456" }
     ```
   - **Event-carried state transfer** (fat event): Contains the full entity state or the relevant subset. Consumer has all the data it needs without calling back. Decoupled, but larger messages and potential data staleness if the entity changes between event production and consumption.
     ```json
     {
       "order_id": "ord_123",
       "customer_id": "cust_456",
       "items": [...],
       "total": "149.99",
       "currency": "USD",
       "shipping_address": {...},
       "placed_at": "2024-01-15T10:30:00Z"
     }
     ```
   - **Delta event** (change event): Contains the old and new values of changed fields. Useful for CDC, audit logging, and consumers that only need to react to specific field changes.
     ```json
     {
       "order_id": "ord_123",
       "changes": {
         "status": { "old": "confirmed", "new": "shipped" },
         "shipped_at": { "old": null, "new": "2024-01-16T14:00:00Z" }
       }
     }
     ```
   - Choose the payload style based on the consumer's needs. Fat events are preferred when decoupling is important and message size is not a concern. Thin events are preferred when message size must be minimal and the source service is always available.

   **Command payloads** — what to do:
   - Include all inputs needed to perform the action. The consumer should not need to fetch additional data from external services to process the command.
     ```json
     {
       "recipient_email": "user@example.com",
       "template": "order_confirmation",
       "template_data": {
         "order_id": "ord_123",
         "customer_name": "Jane Doe",
         "items": [...],
         "total": "$149.99"
       }
     }
     ```

   **Payload rules**:
   - **Do not include sensitive data unnecessarily.** If the consumer does not need the customer's SSN or credit card number, do not include it in the event. Minimize PII in messages — it complicates compliance (GDPR, data retention, encryption requirements on the message broker).
   - **Use consistent field naming** (snake_case recommended for JSON payloads).
   - **Use ISO 8601 for all timestamps**, always in UTC.
   - **Use strings for monetary values** with explicit currency: `"amount": "149.99", "currency": "USD"`.
   - **Include enough context for the consumer to process independently.** If the consumer needs the customer's email to send a notification, include it in the event — do not force the consumer to look it up.

10. **Design message schema governance.** Message schemas are contracts between producers and consumers. Breaking a schema breaks consumers:

    **Schema registry** (recommended for Kafka and Avro/Protobuf-based systems):
    - Use a schema registry (Confluent Schema Registry, AWS Glue Schema Registry, Apicurio) to store, version, and validate message schemas.
    - Producers register schemas before publishing. Consumers retrieve schemas for deserialization.
    - The registry enforces compatibility rules: new schema versions must be compatible with previous versions.

    **Compatibility modes**:
    - **Backward compatible** (recommended default): New consumers can read messages produced with the old schema. Achieved by: only adding optional fields, never removing or renaming fields, never changing field types.
    - **Forward compatible**: Old consumers can read messages produced with the new schema. Achieved by: only removing optional fields.
    - **Full compatible**: Both backward and forward compatible. Most restrictive but safest.
    - **Breaking changes**: Require a new message type (`order.placed.v2`) or a new topic. Coordinate migration across producers and consumers.

    **Schema evolution rules** (apply regardless of whether a registry is used):
    - **Adding a field**: Always make it optional with a default value or explicit null handling. Existing consumers must tolerate unknown fields.
    - **Removing a field**: Stop producing the field first. Verify no consumers depend on it. Remove after all consumers have been updated.
    - **Renaming a field**: Do not rename — it is a breaking change. Add the new name as a new field, produce both old and new names for a migration period, then deprecate the old name.
    - **Changing a field's type**: Do not change — it is a breaking change. Add a new field with the new type.
    - **Consumers must tolerate unknown fields**: A consumer receiving a message with fields it does not recognize must ignore them, not fail.

    **Serialization format selection**:
    - **JSON**: Human-readable, widely supported, schema-less (validation is application-level). Recommended for: moderate throughput, debugging ease, systems without a schema registry.
    - **Avro**: Compact binary format with schema embedded or registry-referenced. Excellent schema evolution support. Recommended for: Kafka-based event streaming with schema registry, high throughput, polyglot environments.
    - **Protocol Buffers**: Compact binary format with strong typing and code generation. Excellent schema evolution. Recommended for: high throughput, strongly typed systems, gRPC integration.
    - **MessagePack**: Compact binary JSON-like format. Faster than JSON, no schema. Recommended for: when JSON is too large/slow but a schema registry is not justified.
    - Choose one format per system (or per technology boundary) and apply consistently.