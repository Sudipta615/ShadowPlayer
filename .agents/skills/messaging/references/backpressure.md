# Phase 12: Backpressure and Flow Control

27. **Design backpressure handling.** When consumers cannot keep up with the production rate, messages accumulate (lag grows). Without explicit backpressure handling, the system will eventually fail (memory exhaustion, message expiry, cascading timeouts):

    **Detection**:
    - **Kafka**: Monitor consumer lag (offset difference between latest produced message and last consumed message). Growing lag indicates consumers are falling behind.
    - **SQS**: Monitor `ApproximateNumberOfMessagesVisible` (queue depth) and `ApproximateAgeOfOldestMessage`. Growing depth or age indicates consumers are falling behind.
    - **RabbitMQ**: Monitor queue depth (`messages_ready`) and consumer utilization.

    **Response strategies**:

    **Strategy 1: Scale consumers (preferred)**:
    - Add more consumer instances to increase throughput. This is the primary response to backpressure.
    - Limit: Kafka partitions cap maximum parallelism. SQS has no practical limit. RabbitMQ scales with more consumers per queue.

    **Strategy 2: Optimize consumer processing**:
    - Profile the consumer — where is time spent? Database writes, API calls, computation?
    - Batch processing: process multiple messages per database transaction.
    - Async I/O: use non-blocking calls for downstream operations.
    - Caching: cache reference data that the consumer looks up frequently.
    - This is a development effort but may provide more sustainable improvement than simply adding instances.

    **Strategy 3: Producer-side flow control**:
    - If the producer can be slowed down, implement backpressure signaling:
      - The producer monitors consumer lag or queue depth and reduces production rate when lag exceeds a threshold.
      - For API-triggered production: return 503 (Service Unavailable) with `Retry-After` header when the downstream queue is saturated.
    - This is appropriate when the producer is an internal service. It is not appropriate when the producer is external (user requests) — you cannot tell users to wait.

    **Strategy 4: Selective message dropping (at-most-once scenarios only)**:
    - If the system can tolerate message loss (metrics, non-critical telemetry), drop messages when the queue exceeds a depth threshold.
    - Never drop messages for business-critical flows. Use this only for explicitly at-most-once data.

    **Strategy 5: Message buffering with overflow**:
    - Buffer messages in a durable store (S3, database) when the primary messaging system is overwhelmed. Process buffered messages during low-traffic periods.
    - Complex to implement correctly. Use only as a last resort for extreme spike handling.