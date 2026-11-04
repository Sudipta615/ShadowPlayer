# Phase 9: Message Ordering and Sequencing

21. **Design ordering guarantees.** Ordering is one of the most misunderstood aspects of messaging. Design it explicitly:

    **Kafka ordering guarantees**:
    - **Within a partition**: Messages are strictly ordered. If message A is produced before message B with the same partition key, consumers will always see A before B.
    - **Across partitions**: No ordering guarantee. Messages in different partitions may be consumed in any order.
    - **Implication**: If per-entity ordering is required, all messages for the same entity must go to the same partition (same partition key). If total ordering is required, use a single partition (but this limits throughput to a single consumer).

    **SQS ordering guarantees**:
    - **Standard queues**: Best-effort ordering. Messages may be delivered out of order. Do not rely on ordering.
    - **FIFO queues**: Strict ordering within a message group. Messages with the same `MessageGroupId` are delivered in the exact order they were sent. Different message groups can be processed in parallel. Maximum throughput: 300 msg/s without batching, up to 70K msg/s with high-throughput FIFO mode.
    - **Implication**: Use `MessageGroupId` as the ordering key (similar to Kafka partition key).

    **RabbitMQ ordering guarantees**:
    - Messages published to a queue are delivered to consumers in FIFO order. However, with multiple consumers and acknowledgment failures (message requeued), ordering can be disrupted.
    - For strict ordering with RabbitMQ: use a single consumer per queue, or use the single-active-consumer feature (only one consumer receives messages at a time, failover to another if the active consumer disconnects).

22. **Design out-of-order message handling.** Even with partition-level ordering, consumers may encounter out-of-order messages in practice (producer retries with different ordering, multi-partition reads, or cross-service event sequences):

    - **Version/sequence checking**: Include a monotonically increasing sequence number or version per entity in the message. Consumer checks: "Is this message's version higher than the last processed version for this entity?" If not, skip it (it is a stale/duplicate message).
    - **Timestamp-based ordering**: Use the event timestamp to detect out-of-order delivery. Consumer processes only if the timestamp is newer than the last processed timestamp for the entity. Caution: clock skew between producers can cause issues — use a logical clock (sequence number) instead of wall clock when precision matters.
    - **Buffering and reordering**: For consumers that require strict ordering across messages from different partitions or sources, buffer incoming messages and sort by sequence number before processing. Release messages in order, holding back messages until gaps are filled (or a timeout expires). Complex to implement correctly — use only when the business logic genuinely requires cross-partition ordering.
    - **Last-write-wins**: For state synchronization use cases, always apply the latest state regardless of message order. The message contains the full entity state, and the consumer unconditionally overwrites with the latest version. This is naturally idempotent and order-independent.