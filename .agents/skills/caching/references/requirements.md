# Phase 1: Caching Requirements Discovery and Justification

1. **Identify the performance problem that caching is intended to solve.** Caching is not a default architectural component — it is a solution to a specific measured problem. Before any caching design, establish:
   - **What is slow or overloaded?** Specific endpoint, query, computation, or service. State the current measured latency (p50, p95, p99) or throughput.
   - **What is the target?** Acceptable latency or throughput. If the user has no target, help define one based on the use case: "For a product listing API called on every page load, a reasonable target is < 50ms p95."
   - **Why is it slow?** Root cause: expensive database query, high computation cost, slow upstream service call, repeated identical work across requests, or throughput exceeding the backend's capacity.
   - **Is caching the right solution?** Before recommending a cache, verify that simpler solutions have been considered:
     - Can the database query be optimized with a better index? (Cheaper and simpler than caching.)
     - Can the computation be simplified or precomputed in the data pipeline?
     - Can the upstream service be made faster or called less frequently by the application logic?
     - Is the bottleneck actually CPU, memory, or network, not I/O?
     If the root cause can be eliminated at the source, that is preferable to adding a caching layer. Caching masks underlying performance problems — it does not fix them.

   State the justification explicitly: "The product catalog query joins 4 tables and takes 180ms p95. Adding a composite index reduced it to 80ms, but the target is < 20ms for this high-frequency endpoint (called 5,000 times/second at peak). The data changes infrequently (catalog updates happen 3-4 times/day). Caching the query result with a 5-minute TTL will reduce p95 to < 5ms and eliminate 99%+ of database load for this endpoint. The 5-minute staleness is acceptable because catalog updates are not time-critical."

2. **Catalog the data to be cached.** For each candidate cache entry, document:
   - **Data source**: Where does the data come from? (Database query, API call, computation, aggregation.)
   - **Access pattern**: How is this data accessed? By which key(s)? How frequently? (Reads per second.)
   - **Data size**: How large is a single cached value? How many distinct values exist? (Total cache memory = count × size.)
   - **Update frequency**: How often does the source data change? Per second, per minute, per hour, per day?
   - **Consistency tolerance**: How stale can the cached data be before it causes user-visible problems or business impact? Seconds, minutes, hours? What happens if stale data is served? (Product shows old price for 30 seconds = acceptable. User sees another user's data = critical bug. Inventory shows item available when it is sold out = causes failed orders.)
   - **Cardinality**: How many unique cache keys will exist? (10 cached objects vs. 10 million cached objects require very different strategies.)
   - **Access distribution**: Is access uniform across keys, or do some keys receive disproportionate traffic (hot keys)? What percentage of requests hit the top 1% of keys?
   - **Lifetime and relevance**: Does the data have a natural expiry? (Session data expires when the session ends. Price quotes expire after a business period. Historical data never changes.)

3. **Identify what must NOT be cached.** Not all data is cache-eligible. Explicitly exclude:
   - **Security-sensitive decisions**: Authorization checks, permission evaluations (unless the cache is invalidated immediately on permission change and the staleness window is provably zero or the consequence is negligible — most systems cannot guarantee this).
   - **Financial balances and real-time inventory**: Data where serving a stale value causes incorrect business outcomes (double-spending, overselling). Exception: if the system tolerates approximate counts with reconciliation.
   - **User-specific highly mutable data**: Data that changes on nearly every request (real-time counters in a game, live typing indicators). Caching data that changes faster than the cache can be invalidated creates negative value.
   - **Data that is as cheap to compute as to cache**: If the database query is already < 1ms and the result is small, caching it adds complexity without meaningful benefit.
   - **Personally identifiable information without controls**: Caching PII in shared caches without proper access scoping, TTL, and encryption creates compliance risk (GDPR right to erasure must include cached copies).

   State the exclusion list and the rationale for each exclusion.
