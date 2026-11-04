# Phase 8: Consumer Design and Scaling

19. **Design consumer architecture.** How consumers are structured directly affects reliability, throughput, and operational complexity:

    **Consumer group design**:
    - Each logical consumer (service that processes a message type) is a consumer group.
    - Multiple instances of the same service share the consumer group — the broker distributes messages/partitions among instances.
    - Different services consuming the same topic use different consumer groups — each group gets all messages independently.
    - Naming convention: `{service-name}.{purpose}` — `inventory-service.stock-updater`, `analytics-service.order-tracker`.

    **Consumer concurrency model**:
    - **Single-threaded per partition/queue** (simplest): One thread processes messages sequentially. Ordering is preserved. Throughput is limited by single-thread performance.
    - **Multi-threaded with ordering** (more complex): Multiple threads process messages, but messages with the same partition key are processed by the same thread (in order). Higher throughput while preserving per-key ordering. Implementation: partition messages by key into thread-specific queues.
    - **Multi-threaded without ordering** (maximum throughput): Multiple threads process messages from a shared buffer. Highest throughput but no ordering guarantees. Use only when ordering is not required.
    - Choose based on the ordering requirement from step 2. Default to single-threaded per partition unless throughput requires otherwise.

    **Consumer processing pattern**:
    - **One-at-a-time**: Process each message individually. Simplest, easiest to debug, lowest throughput.
    - **Micro-batching**: Collect N messages (or messages within a time window), process them as a batch. Higher throughput for operations that benefit from batching (bulk database writes, batch API calls). Complexity: batch failure handling (retry the whole batch or individual messages?).
    - **Recommendation**: Start with one-at-a-time. Switch to micro-batching only when throughput measurements show it is necessary, and when the processing operation genuinely benefits from batching.

    **Consumer lifecycle**:
    - **Graceful shutdown**: On shutdown signal (SIGTERM), stop fetching new messages, wait for in-flight messages to complete processing (with a timeout), commit offsets/acknowledge messages, then exit. This prevents message loss and unnecessary redelivery.
    - **Health checks**: Expose a health endpoint that indicates whether the consumer is actively processing messages. Check: is the message broker connection alive? Is the consumer processing messages or stuck? Has the consumer processed a message within the expected interval?
    - **Backpressure**: If the consumer cannot keep up with the message rate, it should signal backpressure rather than crashing or accumulating an unbounded in-memory queue. Mechanisms: pause fetching (Kafka `pause()`), reduce prefetch count (RabbitMQ), or let the visibility timeout expire (SQS).

20. **Design consumer scaling.** Scale consumers to match the message production rate:

    **Kafka consumer scaling**:
    - Maximum parallelism = number of partitions in the topic. If the topic has 12 partitions, at most 12 consumer instances can share the load within a consumer group.
    - If more parallelism is needed, increase the partition count (but this is a one-way operation and may disrupt ordering for existing keys).
    - Monitor consumer lag (see step 28). If lag is growing, add more consumer instances (up to partition count). If lag persists at maximum instances, optimize processing time per message or increase partitions.

    **SQS consumer scaling**:
    - SQS supports virtually unlimited concurrent consumers. Scale consumers based on queue depth.
    - **Lambda trigger**: SQS can trigger Lambda functions automatically, scaling from 0 to thousands of concurrent invocations. Ideal for variable-load task processing.
    - **EC2/ECS/EKS auto-scaling**: Scale consumer instances based on the `ApproximateNumberOfMessagesVisible` metric. Scale up when queue depth exceeds a threshold (e.g., > 1000 messages). Scale down when queue is consistently empty.

    **RabbitMQ consumer scaling**:
    - Add more consumer instances to the same queue. RabbitMQ distributes messages round-robin across consumers.
    - Configure `prefetch_count` per consumer: how many messages the consumer receives before acknowledging. Lower prefetch = fairer distribution but more round-trips. Higher prefetch = higher throughput but risk of uneven distribution. Start with `prefetch_count = 10-50`.

    **Auto-scaling triggers**:
    - **Primary metric**: Consumer lag (Kafka), queue depth (SQS/RabbitMQ), or processing latency (age of oldest unprocessed message).
    - **Scale-up threshold**: Lag > N messages for > M minutes, or queue depth > N for > M minutes.
    - **Scale-down threshold**: Lag consistently near zero and consumer CPU < 20% for > 10 minutes.
    - **Minimum instances**: At least 2 consumer instances for HA (one can fail while the other processes).
    - **Maximum instances**: Capped at partition count (Kafka) or a sensible limit to prevent downstream system overload.