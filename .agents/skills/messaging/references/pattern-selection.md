# Phase 2: Messaging Pattern Selection

4. **Distinguish between events and commands.** This distinction fundamentally shapes the messaging architecture:

   **Events** (something happened):
   - Represent a fact that occurred in the past: `OrderPlaced`, `PaymentCompleted`, `UserRegistered`.
   - Named in past tense.
   - The producer does not know or care who consumes the event, or how many consumers there are.
   - Events are broadcast: multiple independent consumers can react to the same event.
   - Events are facts — they are never rejected by consumers (a consumer may choose to ignore an event, but it does not "reject" it back to the producer).
   - Events carry data about what happened (event payload), not instructions about what to do.
   - Pattern: **pub/sub** (one-to-many).

   **Commands** (do something):
   - Represent a request to perform an action: `SendEmail`, `ProcessPayment`, `ResizeImage`.
   - Named in imperative form.
   - The producer targets a specific consumer (or a pool of consumers for the same command type).
   - Commands are point-to-point: exactly one consumer should process each command.
   - Commands can be rejected or fail — the producer may need to know the outcome.
   - Commands carry instructions (what to do) and the data needed to do it.
   - Pattern: **work queue** (one-to-one, competing consumers).

   For each flow in the catalog (step 3), classify it as an event or a command. This classification determines the messaging pattern (pub/sub vs. queue) and the technology features needed.

5. **Select the messaging pattern for each flow:**

   **Point-to-point (work queue / task queue)**:
   - One producer, one consumer group (possibly multiple instances competing for messages).
   - Each message is processed by exactly one consumer.
   - Use for: commands, background job processing, task distribution, load leveling.
   - Technologies: SQS, RabbitMQ queues, Redis lists/streams (with consumer groups), Kafka (with a single consumer group).

   **Pub/Sub (publish-subscribe / fan-out)**:
   - One producer, multiple independent consumer groups.
   - Each message is delivered to all subscribed consumer groups. Within each group, the message is processed by exactly one instance.
   - Use for: domain events, event notifications, data synchronization across services, event-driven architecture.
   - Technologies: Kafka (multiple consumer groups on same topic), SNS+SQS (fan-out), RabbitMQ (fanout/topic exchanges), Google Pub/Sub, NATS JetStream.

   **Event streaming (ordered log)**:
   - Messages are stored in a durable, ordered, partitioned log.
   - Consumers read from the log at their own pace, maintaining their own offset/position.
   - Messages are retained for a configurable period regardless of consumption.
   - Consumers can replay from any position (reprocessing, new consumer bootstrapping).
   - Use for: event sourcing, event-driven architecture where replay is needed, data pipeline, change data capture, audit logs.
   - Technologies: Kafka, Pulsar, Kinesis, Redpanda, NATS JetStream.

   **Request-reply (async request-response)**:
   - Producer sends a message and expects a response on a reply queue/topic.
   - Use for: operations where the producer needs the result but does not want to wait synchronously (long-running operations, operations routed through a broker for decoupling).
   - Technologies: RabbitMQ (reply-to header + correlation ID), Kafka (reply topic + correlation ID), or custom implementation on any broker.
   - Caution: This pattern reintroduces coupling and complexity. Prefer pure async (fire-and-forget events/commands with status polling or callbacks) over async request-reply unless there is a strong reason.

   For each flow in the catalog, state the selected pattern and why.
