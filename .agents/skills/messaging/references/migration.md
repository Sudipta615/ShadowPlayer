# Phase 16: Migration and Evolution

36. **Design messaging migration strategies.** Systems evolve — migrating from one messaging technology to another, adding new consumers, or changing message formats:

    **Adding a new consumer to an existing topic/queue**:
    - **Kafka**: Create a new consumer group. The new group reads from the beginning (`auto.offset.reset=earliest`) to process historical messages, or from the latest (`auto.offset.reset=latest`) to process only new messages.
    - **SQS**: Create a new SQS queue subscribed to the same SNS topic. The new queue receives all future messages.
    - **RabbitMQ**: Create a new queue bound to the same exchange. The new queue receives all future messages.
    - **Historical data**: If the new consumer needs to process historical data that is no longer in the topic's retention, backfill from the source database or a data lake.

    **Changing message format (schema migration)**:
    - Follow the schema evolution rules from step 10 (add optional fields, never remove or rename in the same version).
    - For breaking changes:
      1. Create a new topic with the new schema: `orders.placed.v2`.
      2. Update the producer to publish to both `orders.placed` (old) and `orders.placed.v2` (new) during the migration period.
      3. Migrate consumers to the new topic one at a time.
      4. Once all consumers are migrated, stop producing to the old topic.
      5. After the old topic's retention period expires, delete it.

    **Migrating between messaging technologies** (e.g., RabbitMQ to Kafka):
    - **Dual-write migration**: Producer publishes to both old and new systems during migration. Consumers are migrated one at a time to the new system. After all consumers are migrated, stop publishing to the old system.
    - **Bridge migration**: Deploy a bridge/connector that reads from the old system and publishes to the new system (or vice versa). Consumers migrate at their own pace.
    - **Verify**: During migration, compare message counts, processing results, and end-to-end latency between old and new systems.
    - **Rollback plan**: Define how to revert to the old system if the new system has issues. Keep the old system running (read-only) for at least 2 weeks after full migration.