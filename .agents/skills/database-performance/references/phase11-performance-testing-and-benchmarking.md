# Phase 11: Performance Testing and Benchmarking

32. **Design the performance testing strategy.** Performance claims without benchmarks are opinions. Establish a rigorous testing framework:

    **Step 32a: Define the test environment.**
    - The test environment must match production in: database engine version, instance type/size, storage type and IOPS, configuration parameters, and data volume. A test against 10,000 rows tells you nothing about production behavior at 10 million rows.
    - Use anonymized/synthetic data at production scale. Generate realistic data distributions — uniform random data hides hotspot issues.
    - Isolate the test environment from other workloads.

    **Step 32b: Define the workload model.**
    - Catalog the production query mix: what percentage of traffic is each query type? (e.g., 60% order lookups, 20% order list, 10% order creation, 5% search, 5% reporting.)
    - Model the concurrency: how many concurrent database sessions at peak?
    - Model the data distribution: realistic cardinality, skew, and null distributions. If 80% of orders belong to 20% of customers, the test data must reflect this.

    **Step 32c: Define the performance metrics to capture.**
    - Query latency: p50, p95, p99, max for each query type.
    - Throughput: Transactions per second (TPS) and queries per second (QPS).
    - Resource utilization: CPU, memory, disk I/O, connection count during the test.
    - Error rate: Timeouts, deadlocks, connection failures.

    **Step 32d: Execute benchmark types.**
    - **Baseline benchmark**: Measure current performance with the current schema, indexes, and configuration. This is the reference point.
    - **Stress test**: Gradually increase concurrency until performance degrades. Identify the breaking point (the concurrency level where p95 latency exceeds SLO or errors appear). This is the system's current capacity.
    - **Soak test**: Run at expected peak load for 4-8 hours. Look for: memory leaks, connection pool exhaustion, bloat accumulation, replication lag drift, disk space consumption.
    - **Spike test**: Suddenly increase load from normal to 3-5x peak. Verify: connection pool handles the burst, queries don't timeout, autoscaling triggers (if applicable), and the system recovers after the spike.
    - **Regression test**: After each optimization, re-run the baseline benchmark to measure improvement and verify no regressions in other queries.

33. **Use database-specific benchmarking tools.** Select appropriate tools:
    - **pgbench** (PostgreSQL built-in): For basic OLTP benchmarking with customizable scripts. Good for measuring TPS and latency under controlled concurrency.
    - **sysbench**: Multi-database OLTP benchmark with configurable workloads (point selects, range scans, updates). Good for comparing configurations.
    - **HammerDB**: Multi-database TPC-C and TPC-H benchmark tool. Good for standardized workload comparison.
    - **Custom scripts**: For realistic benchmarks, write scripts that execute the actual production query mix at the correct ratios. Use tools like k6, locust, or custom drivers.
    - **`EXPLAIN (ANALYZE, BUFFERS)` with timing**: For micro-benchmarking individual queries. Run 10+ iterations and average to account for caching effects. Run once with a cold cache (`pg_prewarm` can be used to control cache state) and once warm to understand both scenarios.
