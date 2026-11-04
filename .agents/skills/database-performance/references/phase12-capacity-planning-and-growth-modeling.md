# Phase 12: Capacity Planning and Growth Modeling

34. **Perform capacity analysis.** For each production database, establish a capacity model:

    **Current resource utilization**:
    | Resource | Current | Limit | Headroom |
    |---|---|---|---|
    | CPU | 35% avg, 70% peak | 100% | 30% peak headroom |
    | Memory | 12GB used | 16GB total | 4GB headroom |
    | Storage | 250GB used | 500GB provisioned | 50% headroom |
    | IOPS | 2,500 avg, 4,000 peak | 6,000 provisioned | 33% peak headroom |
    | Connections | 80 active | 200 max | 60% headroom |
    | Replication lag | 200ms avg | 5s acceptable | OK |

    **Growth rate calculation**:
    - Data growth: current GB, monthly growth rate, projection at 3, 6, 12 months.
    - Traffic growth: current QPS, monthly growth rate, projection.
    - Connection growth: correlated with application instance count, which correlates with traffic.

    **Identify the first resource to exhaust** (the bottleneck that will hit first):
    - Example: "At the current growth rate of 15GB/month, storage will reach 80% utilization in 4 months. At the current QPS growth rate, CPU will reach 80% peak utilization in 6 months. Storage is the first constraint."

    **Define the scaling plan**:
    - For each resource, define the scaling action and the trigger threshold:
      - Storage: Increase provisioned storage when utilization exceeds 70%. Automate if the cloud provider supports it.
      - CPU: Vertically scale (larger instance) when sustained CPU > 70%. If vertical scaling is exhausted, add read replicas or shard.
      - IOPS: Increase provisioned IOPS or move to a higher storage tier.
      - Connections: Deploy PgBouncer before connection count reaches 80% of max_connections.

35. **Model scaling scenarios.** For anticipated growth events (product launch, seasonal peak, new market):
    - Estimate the traffic multiplier (e.g., 3x current peak).
    - Calculate the resource requirements at that multiplier.
    - Identify which optimizations or scaling actions must be completed before the event.
    - Define a load test that validates the system at the projected load.
    - Example: "Black Friday is projected at 4x normal peak. Current peak CPU is 70% → projected 280% → exceeds single instance. Plan: add 2 read replicas for read offload, pre-warm the cache, pre-provision IOPS to 10,000, increase connection pool sizes. Load test at 5x by October 15."
