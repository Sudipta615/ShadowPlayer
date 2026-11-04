# Phase 14: Messaging Infrastructure Operations

32. **Design Kafka operational procedures** (if Kafka is selected):

    **Topic management**:
    - Create topics via automation (Terraform, CI/CD pipeline), not manually. Define topic configuration in code: partition count, replication factor, retention, cleanup policy.
    - **Replication factor**: 3 for production topics (tolerates 1 broker failure). Never 1 in production. 2 is acceptable for non-critical topics.
    - **Retention**: Define per topic based on requirements. Event logs: 7-30 days. Short-lived commands: 24-48 hours. Audit logs: 90-365 days. Compact topics: infinite retention with compaction.
    - **Cleanup policy**: `delete` (remove messages older than retention period — default for most topics), `compact` (retain only the latest value per key — use for CDC-style state topics, changelogs, KTable backing), `compact,delete` (compact and then delete old segments).
    - `min.insync.replicas = 2` (with replication factor 3): Ensures writes are acknowledged by at least 2 replicas. Combined with `acks=all`, this guarantees no data loss as long as at most 1 broker fails.

    **Consumer group management**:
    - Monitor consumer group state: `STABLE` (healthy), `REBALANCING` (partitions being reassigned — brief processing pause), `DEAD` (no active members — no processing), `EMPTY` (group exists but has no members).
    - **Rebalance minimization**: Use cooperative sticky assignor (step 12). Set `session.timeout.ms` and `heartbeat.interval.ms` appropriately (default session timeout: 45s, heartbeat: 15s for Kafka 3.0+). Consumer must send heartbeats faster than the session timeout.
    - **Offset management**: Use Kafka's internal offset storage (`__consumer_offsets` topic). Commit offsets synchronously after processing for at-least-once. For high-throughput consumers, commit offsets asynchronously in batches, accepting a small window of potential reprocessing.

    **Broker capacity planning**:
    - **Disk**: `disk_needed = (production_rate_bytes/sec × retention_seconds × replication_factor) + 30% overhead`.
    - **Network**: `ingress = production_rate × message_size`. `egress = ingress × (replication_factor - 1 + consumer_group_count)` (each replica and each consumer group generates egress).
    - **CPU**: Kafka is I/O-bound, not CPU-bound for most workloads. CPU becomes relevant with compression and TLS.
    - Monitor and project: track disk usage growth, network utilization, and partition count growth. Plan scaling actions before limits are reached.

33. **Design RabbitMQ operational procedures** (if RabbitMQ is selected):

    **Queue management**:
    - Use quorum queues for all production queues. Classic mirrored queues are deprecated and have known data loss scenarios during network partitions.
    - Set queue limits: `x-max-length` (maximum queue depth — reject or dead-letter overflow messages), `x-max-length-bytes` (maximum queue size in bytes), `x-message-ttl` (per-queue message TTL).
    - Define queue auto-delete and exclusivity settings based on the use case.

    **Cluster management**:
    - Deploy a 3-node cluster for production HA. Quorum queues use Raft consensus across cluster nodes.
    - **Network partition handling**: Configure `cluster_partition_handling` to `pause_minority` (recommended) or `autoheal`. Monitor for partition events.
    - **Memory management**: Set `vm_memory_high_watermark` (default 0.4 — RabbitMQ starts blocking publishers when memory exceeds 40% of available RAM). Monitor memory usage and queue depth to prevent publisher blocking.
    - **Disk alarm**: Set `disk_free_limit` to ensure RabbitMQ has enough free disk for its database and WAL. Default: 50MB. Set to at least 2x the value of `vm_memory_high_watermark` to prevent disk exhaustion.

34. **Design message broker security.**
    - **Authentication**: Enable authentication on the broker. Kafka: SASL/SCRAM or mTLS. RabbitMQ: username/password or x.509 certificates. SQS/SNS: IAM roles.
    - **Authorization**: Restrict which services can publish to and consume from which topics/queues. Kafka: ACLs or RBAC (Confluent). RabbitMQ: vhost permissions and topic permissions. SQS: IAM policies per queue.
    - **Encryption in transit**: Enable TLS for all broker connections. Kafka: configure `security.protocol=SSL` or `SASL_SSL`. RabbitMQ: configure TLS listeners.
    - **Encryption at rest**: Enable disk encryption on broker storage volumes.
    - **Principle of least privilege**: Each service should have credentials that allow access only to the topics/queues it legitimately needs.