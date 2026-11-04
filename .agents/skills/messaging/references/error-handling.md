# Phase 7: Error Handling and Dead Letter Processing

16. **Design retry strategies.** Message processing will fail. Design explicit retry behavior for each failure type:

    **Classify failures**:
    - **Transient failures** (retry will likely succeed): Network timeout, temporary database unavailability, rate limit from downstream service, temporary resource exhaustion. Retry these.
    - **Permanent failures** (retry will never succeed): Invalid message format, business rule violation (order already cancelled), missing required data, deserialization error. Do not retry — send to dead-letter queue.
    - **Indeterminate failures** (unclear if retry will succeed): Downstream service returned 500, database returned an unexpected error. Retry a limited number of times, then treat as permanent failure.

    **Retry policy design**:
    - **Retry count**: 3-5 retries for transient failures. After exhausting retries, route to the dead-letter queue.
    - **Backoff strategy**: Exponential backoff with jitter. Base delay: 1 second. Formula: `delay = min(base_delay * 2^attempt + random_jitter, max_delay)`. Maximum delay: 30 seconds to 5 minutes depending on the use case. Jitter prevents thundering herd when multiple consumers retry simultaneously.
    - **Retry topics (Kafka pattern)**: Instead of delaying within the consumer (which blocks the consumer from processing other messages), publish failed messages to retry topics with increasing delays:
      - `orders.placed.retry-1` (delay: 30 seconds)
      - `orders.placed.retry-2` (delay: 5 minutes)
      - `orders.placed.retry-3` (delay: 30 minutes)
      - After the last retry topic, route to the dead-letter topic.
      - Each retry topic has its own consumer that waits for the delay period before processing. Alternatively, use a delayed message feature if the broker supports it.
    - **RabbitMQ delayed retry**: Use the `x-dead-letter-exchange` and `x-message-ttl` features to create retry queues with delays. Failed messages are published to a retry queue with a TTL; when the TTL expires, the message is routed to the original queue via DLX.
    - **SQS retry**: SQS natively supports retry via visibility timeout. If the consumer does not delete the message, it becomes visible again after the visibility timeout. Configure `maxReceiveCount` on the queue's redrive policy to route to DLQ after N failures.

17. **Design dead-letter queue (DLQ) processing.** Messages that fail all retries must go somewhere — they cannot be silently dropped:

    **DLQ design**:
    - Every queue/topic that processes messages must have a corresponding DLQ:
      - Kafka: `orders.placed.dlq`
      - SQS: `notification-send-email-dlq`
      - RabbitMQ: Dead-letter exchange + dead-letter queue per source queue.
    - **Enrich DLQ messages with failure context**: When routing to DLQ, add metadata:
      - Original message (complete, unmodified).
      - Error message and stack trace.
      - Number of retry attempts.
      - Timestamp of last failure.
      - Consumer instance that failed.
      - Original topic/queue.
    - **DLQ retention**: Retain DLQ messages for at least 14-30 days. This provides time for investigation and manual reprocessing.

    **DLQ monitoring and alerting**:
    - **Alert on any message arriving in a DLQ** (warning level). A DLQ message indicates a processing failure that exhausted retries — it requires human attention.
    - **Alert on DLQ depth increasing** (critical level if sustained). Growing DLQ depth indicates a systemic processing problem, not a one-off failure.
    - **DLQ dashboard**: Display DLQ depth per queue, message age distribution, and failure reason distribution.

    **DLQ reprocessing**:
    - Design a mechanism to replay DLQ messages back to the original queue after the root cause is fixed:
      - Manual: An admin tool that reads DLQ messages and republishes them to the source queue.
      - Automated: A DLQ reprocessor service that monitors the DLQ and retries messages on a schedule (e.g., every hour). Include a max-replay-count to prevent infinite loops.
    - Before reprocessing, fix the root cause. Replaying messages into a system that still has the bug will just send them back to the DLQ.
    - **Reprocessing must be idempotent**: Since the original message may have been partially processed before failing, reprocessing must not create duplicate side effects.

18. **Design poison message handling.** A poison message is a message that causes the consumer to crash or hang every time it is processed. Without explicit handling, a poison message can block the entire queue (the consumer processes it, crashes, the message is redelivered, the consumer processes it again, crashes again — infinite loop):

    - **Detection**: Track per-message delivery count. If a message has been delivered more than N times (e.g., 3), treat it as a potential poison message.
    - **Isolation**: Route poison messages to the DLQ immediately without further retry.
    - **Consumer resilience**: Wrap message processing in a try-catch that catches all exceptions. Never let an unhandled exception crash the consumer process — log the error, nack the message, and continue processing other messages.
    - **Timeout**: Set a processing timeout per message. If processing takes longer than the expected maximum (e.g., 5x the normal processing time), kill the processing, nack the message, and log a warning. This prevents messages that cause infinite loops or deadlocks from blocking the consumer.
    - **SQS**: Set `maxReceiveCount` on the redrive policy. After `maxReceiveCount` deliveries without deletion, SQS automatically moves the message to the DLQ.
    - **Kafka**: Track delivery attempts in a header or external store. If attempts exceed the threshold, produce to DLQ topic and commit the offset (skip the message).
    - **RabbitMQ**: Use `x-delivery-count` header (available in quorum queues) or track in application code. Route to DLQ after exceeding the threshold.