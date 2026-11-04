# Phase 15: Testing Messaging Systems

35. **Design messaging test strategy.** Asynchronous messaging systems are harder to test than synchronous APIs — test failures may be timing-dependent, and end-to-end flows span multiple services:

    **Unit tests**:
    - Test message serialization/deserialization: verify that producing and consuming the same message type yields identical data.
    - Test consumer processing logic in isolation: given a message, verify the correct side effects (database changes, API calls, events produced). Mock the message broker.
    - Test idempotency: process the same message twice and verify the same result (no duplicate side effects).
    - Test error handling: simulate transient and permanent failures, verify correct retry and DLQ routing behavior.

    **Integration tests**:
    - Use an embedded or containerized message broker (Testcontainers for Kafka, RabbitMQ). Produce a message, verify the consumer processes it, and verify the expected side effects.
    - Test the full produce → consume → process → acknowledge cycle.
    - Test DLQ routing: produce a message that will cause a permanent failure, verify it arrives in the DLQ with correct metadata.
    - Test ordering: produce multiple messages with the same partition key, verify they are processed in order.
    - Test consumer failure and recovery: kill the consumer mid-processing, restart it, verify the message is reprocessed and no messages are lost.

    **Contract tests**:
    - Define message schemas as contracts between producers and consumers.
    - Test that the producer generates messages conforming to the contract schema.
    - Test that the consumer can deserialize and process messages conforming to the contract schema.
    - Test backward compatibility: the consumer can process messages in both the old and new schema versions.

    **End-to-end tests**:
    - Test critical business flows end-to-end in a staging environment: trigger the initial action (e.g., place an order via API), verify that all downstream consumers process successfully (inventory updated, payment processed, notification sent).
    - Use correlation IDs to trace the flow. Assert on final state, not intermediate messages — intermediate messages are implementation details.

    **Chaos tests** (for mature systems):
    - Kill a broker node during production/consumption. Verify: no message loss (with `acks=all`), consumers recover after leader election, and lag is temporary.
    - Introduce network latency between application and broker. Verify: timeouts are handled correctly, retries succeed, and no duplicate processing beyond idempotent handling.
    - Kill a consumer instance during processing. Verify: the message is redelivered to another instance, no data loss, and no duplicate side effects.