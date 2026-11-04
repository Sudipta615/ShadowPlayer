# Phase 13: Messaging Observability

28. **Design messaging monitoring.** Messaging systems are inherently harder to observe than synchronous systems — messages are invisible between production and consumption. Design comprehensive observability:

    **Producer metrics** (per topic/queue):
    - **Publish rate**: Messages published per second. Track trends for capacity planning.
    - **Publish latency**: p50, p95, p99 time from publish call to broker acknowledgment. Alert if p99 exceeds 100ms (Kafka), 50ms (RabbitMQ/SQS).
    - **Publish errors**: Failed publish attempts per second. Alert on any sustained non-zero error rate.
    - **Message size**: Average and p99 message size. Track for capacity planning and bandwidth estimation.

    **Consumer metrics** (per consumer group / queue):
    - **Consumer lag** (critical metric):
      - Kafka: `consumer_lag = latest_offset - committed_offset` per partition. The single most important Kafka operational metric. Alert when total lag exceeds threshold (e.g., > 10,000 messages for > 5 minutes).
      - SQS: `ApproximateNumberOfMessagesVisible` (queue depth) and `ApproximateAgeOfOldestMessage`.
      - RabbitMQ: `messages_ready` (queue depth).
    - **Consumption rate**: Messages consumed per second. Should approximately equal production rate at steady state.
    - **Processing latency**: Time from message receipt to processing completion. p50, p95, p99. Alert if p99 exceeds the latency target defined in the flow catalog.
    - **Processing errors**: Failed processing attempts per second. Alert on any sustained non-zero error rate.
    - **End-to-end latency**: Time from message production to processing completion. This is the latency the business cares about. Calculate: `processing_timestamp - message.timestamp`. Alert if it exceeds the flow's latency target.
    - **Redelivery rate**: Messages redelivered (indicating processing failure and retry). High redelivery rate indicates consumer instability.

    **Broker metrics** (per broker/cluster):
    - **Kafka**: Broker CPU, memory, disk I/O, disk space, under-replicated partitions, ISR (in-sync replica) count, request handler idle percentage, network throughput.
    - **RabbitMQ**: Node CPU, memory, disk space, file descriptors, Erlang process count, queue count, connection count, channel count, cluster partition state.
    - **SQS**: Managed — monitor `NumberOfMessagesSent`, `NumberOfMessagesReceived`, `NumberOfMessagesDeleted`, `ApproximateNumberOfMessagesVisible`, `ApproximateAgeOfOldestMessage`.

    **Dead-letter queue metrics**:
    - DLQ depth per queue.
    - DLQ message arrival rate.
    - DLQ message age distribution.
    - Alert on any DLQ message arrival (warning). Alert on growing DLQ depth (critical).

29. **Design distributed tracing for asynchronous flows.** Tracing asynchronous flows is fundamentally different from tracing synchronous HTTP calls:

    **Trace context propagation**:
    - Inject the trace context (OpenTelemetry `traceparent` / W3C Trace Context) into the message envelope (as a header or envelope field) when producing the message.
    - Extract the trace context from the message when consuming, and create a new span that is a child of (or follows from) the producer's span.
    - This links the producer and consumer spans in the distributed trace, enabling end-to-end visibility of the asynchronous flow.

    **Span design for messaging**:
    - **Producer span**: `PRODUCE {topic/queue}`. Attributes: `messaging.system` (kafka/rabbitmq/sqs), `messaging.destination` (topic/queue name), `messaging.message_id`, `messaging.partition` (Kafka).
    - **Consumer span**: `CONSUME {topic/queue}`. Attributes: same as producer, plus `messaging.consumer_group`, `messaging.processing_duration`.
    - **Processing span**: Child of the consumer span, covering the actual business logic execution. Include spans for downstream calls (database, API) within the processing.

    **Correlation across async boundaries**:
    - Use `correlation_id` (from the message envelope, step 8) to link all messages in a business workflow. When viewing traces, filter by `correlation_id` to see the entire flow: initial API request → event published → consumer A processing → event published → consumer B processing.
    - Include `correlation_id` in all log entries within the consumer's processing context.

30. **Design messaging dashboards.** Build and maintain:

    **Dashboard 1: Messaging Health Overview**
    - Total publish rate across all topics/queues.
    - Total consumption rate.
    - Aggregate consumer lag / queue depth.
    - DLQ depth across all DLQs.
    - End-to-end latency p95 for critical flows.
    - Error rate (publish + processing).

    **Dashboard 2: Per-Flow Detail** (one panel group per flow from the catalog)
    - Production rate.
    - Consumption rate.
    - Consumer lag trend.
    - Processing latency percentiles.
    - Error rate and DLQ arrivals.
    - Consumer instance count and health.

    **Dashboard 3: Broker Infrastructure** (Kafka / RabbitMQ specific)
    - Per-broker CPU, memory, disk, network.
    - Partition distribution and leader balance (Kafka).
    - Under-replicated partitions (Kafka) — alert on any > 0.
    - Queue depth per queue (RabbitMQ).
    - Connection and channel count (RabbitMQ).

31. **Design messaging alerting.** Define actionable alerts:

    **Critical (page — requires immediate response)**:
    - Consumer lag > threshold for > 5 minutes (threshold based on flow SLA — e.g., if SLA is < 30s processing, alert when lag represents > 30s of messages at current consumption rate).
    - DLQ depth increasing for > 10 minutes (indicates systemic processing failure).
    - Broker under-replicated partitions > 0 for > 5 minutes (data durability risk).
    - Broker disk space < 15% free (risk of broker halt).
    - Publish error rate > 1% for > 2 minutes.
    - Consumer group has zero active members for > 2 minutes (no processing happening).
    - End-to-end message latency exceeds SLA for critical flows.

    **Warning (ticket — investigate within business hours)**:
    - Consumer lag growing but not yet critical.
    - DLQ message arrived (any single message — indicates a processing failure worth investigating).
    - Processing error rate > 0.1% for > 15 minutes.
    - Consumer rebalances occurring frequently (> 3 in 10 minutes — indicates unstable consumers).
    - Broker CPU > 70% sustained for > 15 minutes.
    - Topic/queue approaching retention limit (messages may be dropped).

    Every critical alert must have a linked runbook.