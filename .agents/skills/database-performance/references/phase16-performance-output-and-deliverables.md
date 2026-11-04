# Phase 16: Performance Output and Deliverables

45. **Produce performance engineering deliverables.** At the conclusion of every performance engagement, produce:

    - **Performance assessment summary**: Current state of database performance in measurable terms — key metrics, identified bottlenecks, and severity classification.
    - **Root cause analysis**: For each identified performance problem, the chain from symptom → diagnosis → root cause, with supporting evidence (execution plans, metrics, query statistics).
    - **Optimization plan**: A prioritized list of recommended optimizations, ordered by impact (estimated improvement) and effort (implementation complexity). For each optimization:
      - What to change (specific index, query rewrite, configuration change).
      - Why it will help (linked to root cause and specific access pattern).
      - Expected improvement (e.g., "query latency should drop from 1.2s to < 50ms based on index elimination of sequential scan").
      - Risk and rollback plan.
    - **Before/after measurements**: For each optimization applied, the measured performance before and after, proving the improvement.
    - **Capacity forecast**: When the current architecture will hit resource limits at the projected growth rate, and what scaling actions are needed and when.
    - **Monitoring and alerting recommendations**: Specific metrics, thresholds, and dashboards to establish for ongoing performance governance.
    - **Open items**: Optimizations deferred, investigations requiring more data, and architectural changes needed for long-term scalability.

### Cross-Cutting Rules (Apply Throughout All Phases)

46. **Measure before optimizing, measure after optimizing.** Never apply an optimization without establishing a baseline measurement and verifying improvement with a post-optimization measurement. Optimizations applied without measurement are superstition, not engineering. If you cannot measure the before and after, you cannot claim an improvement.

47. **Optimize the most impactful query first.** Use total execution time (frequency × average duration) as the prioritization metric, not individual query latency. A query that runs 100,000 times per hour at 50ms each consumes 10x more resources than a query that runs once per hour at 5 seconds.

48. **Treat every index as a cost, not just a benefit.** Each index speeds up specific reads but slows down every write and consumes storage and memory. An index must justify its existence by serving a specific, measured access pattern. Unused indexes must be removed. The optimal number of indexes is the minimum that satisfies all critical read access patterns — not one more.

49. **Configuration tuning is not a substitute for query optimization.** Increasing `shared_buffers` or `work_mem` can mask problems but does not fix them. A sequential scan on a 50-million-row table is a bug whether the table is cached in memory or not — the fix is an index, not more RAM. Always optimize queries and indexes first, then tune configuration.

50. **State tradeoffs for every recommendation.** Never recommend an optimization without stating what it costs. Format: "Adding index `(customer_id, status, created_at)` will reduce order list query latency from 800ms to ~10ms, but will add ~15% overhead to order INSERT operations and consume approximately 2GB of storage. This is acceptable because reads outnumber writes 50:1 on this table and 2GB is well within storage headroom."

51. **Prefer reversible optimizations.** Indexes can be dropped. Configuration changes can be reverted. Query rewrites can be rolled back. Denormalization and schema changes are harder to reverse. When multiple approaches can solve a problem, prefer the one that is easiest to undo if the results are not as expected.

52. **Performance is a continuous practice, not a project.** One-time optimization degrades as data grows, traffic patterns change, and new queries are added. Establish ongoing monitoring, regular performance reviews, and regression prevention as permanent engineering practices, not as occasional firefighting exercises.
