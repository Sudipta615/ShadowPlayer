# Phase 5: Topic and Queue Architecture

11. **Design the topic/queue topology.** How messages are organized into topics (Kafka), queues (SQS, RabbitMQ), or subjects (NATS) directly affects routing, ordering, consumer isolation, and operational management:

    **Topic design for event streaming (Kafka, Kinesis, Pulsar)**:
    - **One topic per event type** (recommended): `orders.placed`, `orders.shipped`, `payments.completed`, `users.registered`. Each topic contains one type of event.
      - Advantages: Consumers subscribe only to the event types they need. Schema per topic is consistent. Ordering within a topic is meaningful (all events of the same type).
      - Disadvantages: Many topics in a large system (manageable with proper naming and automation).
    - **One topic per domain/entity** (alternative): `orders` (contains `order.placed`, `order.shipped`, `order.cancelled`). All events for the entity are in one topic.
      - Advantages: Fewer topics. Per-entity ordering across event types is preserved (important if consumers need to process `order.placed` before `order.shipped` for the same order).
      - Disadvantages: Consumers receive events they do not need (must filter). Schema varies per message type within the topic (requires a `type` discriminator field).
    - **Decision**: Use one topic per event type when consumers are interested in specific event types and per-entity ordering across types is not required. Use one topic per entity when per-entity ordering across event types is required and consumers process all events for an entity.

    **Topic naming conventions**:
    - Use a hierarchical, dot-separated convention: `{domain}.{entity}.{event}` or `{domain}.{entity}.{version}`.
    - Examples: `orders.placed`, `payments.completed`, `users.profile.updated`, `inventory.stock.adjusted`.
    - Include environment prefix in non-production: `staging.orders.placed`, `dev.orders.placed`. In production, omit the prefix or use `prod.`.
    - Define the naming convention in a documented standard and enforce it in topic creation automation.

    **Queue design for task processing (SQS, RabbitMQ)**:
    - **One queue per consumer service per task type**: `notification-service.send-email`, `image-processor.resize`. Each consumer owns its queue.
    - **Separate queues by priority** (if priority processing is needed): `orders.process.high-priority`, `orders.process.normal`. Consumers prioritize the high-priority queue.
    - **Separate queues by processing characteristics**: If some messages are fast to process (< 100ms) and others are slow (> 10s), use separate queues to prevent slow messages from blocking fast ones.

    **RabbitMQ exchange and binding design**:
    - **Fanout exchange**: Broadcast to all bound queues. Use for: event fan-out where every consumer gets every message.
    - **Direct exchange**: Route to queues by exact routing key match. Use for: command routing to specific consumers.
    - **Topic exchange**: Route by routing key pattern matching (`order.*.completed`, `order.placed.#`). Use for: flexible routing where consumers subscribe to patterns.
    - **Headers exchange**: Route by message header attributes. Use for: complex routing based on multiple message properties.
    - Recommendation: Use topic exchanges as the default for event routing (most flexible). Use direct exchanges for command routing. Use fanout only for simple broadcast with no filtering.

12. **Design Kafka partitioning strategy.** Partition design is critical in Kafka — it determines ordering, parallelism, and throughput:

    **Partition key selection**:
    - The partition key determines which partition a message is assigned to. All messages with the same partition key go to the same partition, guaranteeing ordering within that key.
    - **Choose the partition key based on the ordering requirement** (from step 2):
      - Per-order ordering: `partition_key = order_id`.
      - Per-customer ordering: `partition_key = customer_id`.
      - Per-tenant ordering: `partition_key = tenant_id`.
      - No ordering required: Use round-robin (no partition key) or random key for even distribution.
    - **Cardinality matters**: The partition key must have high cardinality (many distinct values) to distribute messages evenly across partitions. A partition key with 5 distinct values and 100 partitions means 95 partitions are empty — this wastes resources and creates hot partitions.
    - **Hot partition risk**: If 50% of messages have the same partition key (e.g., one very large customer), that partition's consumer handles 50% of the load while other consumers are idle. Monitor partition lag distribution. If hot partitions are a problem, consider: a composite partition key (`customer_id + sub_key`), sharding within the entity, or accepting uneven load with auto-scaling consumers.

    **Partition count**:
    - Start with a moderate partition count: 6-12 partitions for low-throughput topics, 30-100 for high-throughput topics.
    - **Partitions = max consumer parallelism**: The number of partitions is the maximum number of consumers that can process in parallel within a consumer group. If a topic has 12 partitions, at most 12 consumer instances can share the load.
    - More partitions = more parallelism, but also more overhead (memory, file handles, leader election time, rebalance time).
    - **Partitions can be increased but never decreased** in Kafka. Start with enough for projected peak parallelism. Increasing partitions later breaks ordering guarantees for existing keys (keys may be reassigned to different partitions).
    - Rule of thumb: `target_partitions = max(expected_peak_consumer_instances, ceil(peak_throughput_MB_per_sec / 10))`. Adjust based on measurement.

    **Partition assignment strategy**:
    - **Range assignment** (default): Partitions are assigned to consumers in order. Can lead to uneven distribution if topic count varies.
    - **Round-robin assignment**: Partitions are distributed evenly across consumers. More balanced.
    - **Sticky assignment** (recommended): Minimizes partition reassignment during rebalances. Consumers keep their current partitions as much as possible.
    - **Cooperative sticky** (recommended for Kafka 2.4+): Incremental rebalancing — only affected partitions are reassigned, not all partitions. Reduces rebalance disruption.