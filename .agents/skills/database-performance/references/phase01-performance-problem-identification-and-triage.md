# Phase 1: Performance Problem Identification and Triage

1. **Establish the performance problem statement.** Before any analysis, define the problem in measurable terms. If the user says "the database is slow," that is not a problem statement — it is a symptom. Ask and clarify until you have:
   - **What operation is slow?** Specific query, endpoint, workflow, or batch job.
   - **How slow is it?** Current measured latency (p50, p95, p99) or throughput.
   - **What is the target?** Acceptable latency or throughput. If the user has no target, help them define one: "For this user-facing endpoint, a reasonable target is < 100ms p95. Your current p95 is 1.2 seconds. That's a 12x gap."
   - **When did it start?** Was this always slow, or did it degrade recently? If recent, what changed? (Deployment, data growth, traffic increase, configuration change, infrastructure change.)
   - **How consistent is it?** Always slow, intermittently slow (time of day? specific queries? specific data?), or degrading over time?
   - **What is the impact?** User-facing latency, background job timeout, replication lag, cascading failures to other services.

   State the problem in a structured format: "The `GET /orders` endpoint has a p95 latency of 1.2 seconds, up from 200ms two weeks ago. The target is < 200ms p95. The orders table grew from 5M to 12M rows in the last month due to a partner integration. This affects all customer-facing order history views."

2. **Confirm the database is the actual bottleneck.** Before diving into database optimization, verify that the database is where time is being spent:
   - **Application-level tracing**: Check distributed traces (OpenTelemetry, Datadog APM, New Relic) to confirm the database span is the dominant portion of the total request time. If the application spends 50ms in the database and 900ms in application code, the database is not the bottleneck.
   - **Network latency**: Check if the latency is in the network between the application and database (ping, traceroute, connection establishment time). Misplaced infrastructure (application and database in different availability zones or regions) is a common source of latency.
   - **Connection pool wait time**: Check if the latency is in waiting for a connection from the pool, not in query execution. If connection pool wait time is high, the problem is connection management (see Phase 6), not query performance.
   - **N+1 query patterns**: Check if the application is executing hundreds of individual queries per request instead of one efficient query. This is an application code problem, not a database problem — though it manifests as database load.
   - **ORM-generated queries**: Check if the ORM is generating inefficient SQL (unnecessary joins, missing eager loading, fetching entire rows when only a few columns are needed). Capture the actual SQL being executed.

   State the finding: "Confirmed: the database query accounts for 85% of the endpoint's total response time. The bottleneck is a sequential scan on the `orders` table. " or "The database query itself executes in 15ms, but the application makes 47 separate queries per request due to N+1 loading. The fix is in the application code, not the database."

3. **Gather the diagnostic baseline.** Before making any changes, collect the current state:
   - **Database version and configuration**: Exact database engine and version, instance type/size, storage type (SSD/HDD/network-attached), allocated memory, CPU cores.
   - **Current resource utilization**: CPU utilization (average and peak), memory utilization (total, shared buffers usage, OS cache), disk I/O (read/write IOPS, I/O wait, throughput), disk space utilization.
   - **Connection state**: Total connections, active connections, idle connections, waiting connections vs. max_connections.
   - **Key database metrics**: Cache hit ratio, transactions per second, tuple read/write rates, temporary file usage, dead tuple count, last vacuum timestamps.
   - **Query statistics**: Top queries by total execution time (from `pg_stat_statements` or equivalent). Top queries by call count. Top queries by mean execution time.
   - **Replication status** (if applicable): Replication lag, replay lag, WAL send/receive positions.
   - **Active locks and waits**: Currently held locks, waiting locks, long-running transactions.

   This baseline is the reference point for measuring the impact of any optimization. Without it, you cannot prove an optimization worked.
