# Phase 3: Messaging Technology Selection

6. **Select the messaging technology based on the requirements catalog.** Match the technology to the specific requirements — not to popularity, not to familiarity, and not to a single feature:

   **Apache Kafka** (and Kafka-compatible: Redpanda, Amazon MSK, Confluent Cloud):
   - **Architecture**: Distributed, partitioned, replicated commit log. Messages are appended to partitions and retained for a configurable period. Consumers track their own offset.
   - **Select when**:
     - High throughput is required (100K+ messages/second).
     - Message ordering per partition key is required.
     - Message replay and retention are required (new consumers need historical data, reprocessing after bugs).
     - Multiple independent consumer groups need to read the same messages.
     - Event streaming / event-driven architecture with durable event log.
     - Data pipeline use cases (CDC, ETL, log aggregation).
   - **Strengths**: Extreme throughput, durable ordered log, consumer independence, ecosystem (Kafka Connect, Kafka Streams, ksqlDB), mature operational tooling.
   - **Weaknesses**: Operational complexity (ZooKeeper dependency in older versions, partition management, broker configuration), no built-in per-message routing or filtering (consumers read entire partitions), no native delayed/scheduled messages, no per-message TTL (retention is per-topic), higher latency floor than purpose-built queues (~5-50ms vs. ~1ms for RabbitMQ), message ordering only within a partition (not across partitions).
   - **Managed options**: Confluent Cloud (fully managed, schema registry, connectors), Amazon MSK (managed Kafka), Redpanda (Kafka-compatible, no ZooKeeper, simpler operations).
   - **Do not select Kafka when**: You need simple task queuing with individual message acknowledgment, when message routing/filtering per consumer is a primary requirement, when throughput is low (< 1000 msg/s) and operational simplicity matters more, or when the team has no Kafka operational experience and the use case does not justify the learning investment.

   **RabbitMQ** (and AMQP-compatible brokers):
   - **Architecture**: Message broker with exchanges, queues, and bindings. Exchanges route messages to queues based on routing rules. Consumers pull from queues. Messages are removed from the queue after acknowledgment.
   - **Select when**:
     - Complex routing is needed (route messages to different queues based on message attributes, headers, or routing keys).
     - Per-message acknowledgment and redelivery is needed (consumer fails → message returns to queue).
     - Priority queues, delayed messages, or message TTL per-message are needed.
     - Request-reply pattern with correlation IDs.
     - Low latency per-message delivery (< 1ms is achievable).
     - Moderate throughput (1K-50K msg/s per node).
     - The team values protocol standards (AMQP 0-9-1) and broad client library support.
   - **Strengths**: Flexible routing (direct, topic, fanout, headers exchanges), per-message acknowledgment, priority queues, delayed message plugin, message TTL, dead-letter exchanges (built-in), management UI, mature and well-understood.
   - **Weaknesses**: Not designed for message replay (messages are deleted after consumption), no built-in consumer offset tracking (messages are consumed and gone), lower throughput ceiling than Kafka for high-volume streaming, clustering for HA adds operational complexity (split-brain scenarios with network partitions, quorum queues mitigate this).
   - **Use quorum queues** (not classic mirrored queues) for production durability and HA. Quorum queues use Raft consensus and are the recommended queue type in modern RabbitMQ.
   - **Do not select RabbitMQ when**: You need message replay/retention, when you need multiple independent consumer groups reading the same messages without separate exchanges/bindings, when throughput exceeds 50K msg/s, or when you need a durable event log.

   **Amazon SQS** (Simple Queue Service):
   - **Architecture**: Fully managed, serverless message queue. Standard queues (at-least-once, best-effort ordering) and FIFO queues (exactly-once processing, strict ordering within message groups).
   - **Select when**:
     - Simple task queue / work distribution on AWS.
     - Operational simplicity is the top priority (zero infrastructure management, auto-scaling, no capacity planning).
     - Integration with AWS services (Lambda triggers, SNS fan-out, EventBridge).
     - Moderate throughput (standard queues: virtually unlimited; FIFO queues: 300-3000 msg/s per queue without batching, up to 70K msg/s with high-throughput mode).
     - The team does not want to operate messaging infrastructure.
   - **Strengths**: Zero operational overhead, automatic scaling, dead-letter queue support, message visibility timeout (built-in redelivery on consumer failure), message delay (up to 15 minutes), long polling, pay-per-use pricing, extremely reliable (backed by S3-level durability).
   - **Weaknesses**: No message replay (messages are deleted after consumption and visibility timeout), no pub/sub (use SNS+SQS for fan-out), message size limit (256KB, or 2GB with Extended Client Library via S3), FIFO queue throughput limits, no built-in complex routing (use SNS message filtering), AWS-only.
   - **SNS + SQS for pub/sub fan-out**: SNS topic → multiple SQS queues. Each SQS queue is an independent consumer. SNS handles fan-out, SQS handles reliable consumption with per-consumer acknowledgment. This is the standard AWS pattern for event-driven fan-out and is recommended over direct SQS-to-SQS patterns.
   - **Do not select SQS when**: You need message replay/retention, when you need ordered streaming across high-throughput topics, when you are not on AWS, or when you need sub-millisecond latency.

   **Amazon SNS** (Simple Notification Service):
   - **Architecture**: Fully managed pub/sub service. Publishers send to topics. Subscribers receive messages (SQS queues, Lambda functions, HTTP endpoints, email, SMS).
   - **Select when**: Fan-out from one event to multiple consumers on AWS. Message filtering per subscriber (filter by message attributes). Integration with SQS, Lambda, or HTTP endpoints.
   - **Strengths**: Zero operational overhead, message filtering per subscription, fan-out to diverse subscriber types, FIFO topics for ordered fan-out.
   - **Weaknesses**: No message retention (fire-and-forget to subscribers), no consumer offset tracking, limited message size (256KB). Always pair with SQS for reliable consumption.

   **Google Cloud Pub/Sub**:
   - **Architecture**: Fully managed, serverless pub/sub service with message retention and replay. Topics and subscriptions. Each subscription is an independent consumer with its own cursor.
   - **Select when**: GCP environment. Need managed pub/sub with message replay (up to 7 days retention). Need serverless scaling. Need dead-letter topics. Need ordering within ordering keys.
   - **Strengths**: Fully managed, auto-scaling, message retention and replay (unique among managed pub/sub services), ordering keys, dead-letter topics, exactly-once delivery (within Dataflow), global availability.
   - **Weaknesses**: GCP-only, higher latency than Kafka for extreme throughput scenarios, ordering is per ordering key (not global).

   **Azure Service Bus**:
   - **Architecture**: Fully managed enterprise message broker with queues and topics/subscriptions. Supports sessions (ordered message groups), scheduled messages, message deferral, and transactions.
   - **Select when**: Azure environment. Enterprise messaging patterns (sessions, transactions, scheduled delivery, duplicate detection). Need managed service with rich messaging features.
   - **Strengths**: Sessions (ordered processing per session ID), scheduled messages, duplicate detection (built-in), dead-letter queues, transactions across multiple entities, rich management API.
   - **Weaknesses**: Azure-only, pricing can be significant at high throughput.

   **NATS / NATS JetStream**:
   - **Architecture**: Lightweight, high-performance messaging system. Core NATS is pure pub/sub (fire-and-forget, no persistence). JetStream adds persistence, at-least-once delivery, consumer groups, and replay.
   - **Select when**: Need extremely low latency (sub-millisecond). Need lightweight, operationally simple messaging. Edge computing or IoT scenarios. Need both pub/sub and queue semantics in one system.
   - **Strengths**: Extremely fast, simple deployment (single binary), small resource footprint, supports pub/sub, request-reply, and queue groups natively. JetStream adds persistence and delivery guarantees.
   - **Weaknesses**: Smaller ecosystem than Kafka or RabbitMQ, fewer managed service options, less mature tooling for large-scale production operations.

   **Redis Streams**:
   - **Architecture**: Append-only log data structure within Redis, with consumer groups and acknowledgment.
   - **Select when**: Already using Redis, need simple streaming/queuing without adding another infrastructure component, moderate throughput (< 50K msg/s), need consumer groups with acknowledgment.
   - **Strengths**: No additional infrastructure if Redis is already deployed, consumer groups with individual acknowledgment, message retention with configurable trimming, low latency.
   - **Weaknesses**: Limited by Redis single-thread performance, no built-in schema registry, no native partitioning across nodes (Redis Cluster shards by key, not by stream partitioning), persistence depends on Redis persistence configuration (can lose data on crash without AOF).
   - **Do not select for**: High-throughput event streaming, mission-critical messaging where durability is paramount, or complex routing requirements.

7. **Justify the selection explicitly.** For each technology choice:
   - State which flows from the catalog (by flow ID) this technology serves.
   - State what the technology provides (specific capabilities matched to specific requirements).
   - State what the technology costs (operational complexity, limitations, expertise required, infrastructure cost).
   - State what alternatives were considered and why they were rejected.
   - State under what conditions the choice should be reconsidered.
