# Phase 8: Cache Warming and Cold Start

19. **Design cache warming strategy.** A cold cache (empty or after restart) causes every request to hit the data source, creating a load spike ("cold start thundering herd"):

    **Strategy 1: Passive warming (lazy loading)** — accept the cold start:
    - Cache populates naturally as requests arrive. Each cache miss loads from the source and populates the cache.
    - Acceptable when: traffic ramps up gradually (not a sudden spike), the data source can handle the temporary increase in load, and cache miss latency is acceptable for the first few minutes.
    - Risk: If traffic is high immediately (e.g., after a deployment during peak hours), the data source may be overwhelmed.

    **Strategy 2: Active warming (preloading)** — populate the cache before serving traffic:
    - Before an application instance starts accepting requests, preload critical cache entries:
      - Identify the "hot set" — the most frequently accessed keys. Use production access logs or analytics to identify the top N keys by access frequency.
      - Load these keys from the data source and populate the cache.
      - Start accepting traffic only after warming is complete (Kubernetes readiness probe gates traffic until warming finishes).
    - **Warming rate**: Load keys gradually, not all at once. Use a rate limiter on the warming process (e.g., 100 keys/second) to avoid overwhelming the data source. The warming time for N keys at R keys/second is N/R seconds — ensure this fits within the deployment timeline.
    - **Warming on schedule**: For predictable traffic patterns (e.g., business hours start at 9 AM), trigger cache warming at 8:45 AM.
    - **Warming from a replica**: Load data from a database read replica rather than the primary, to avoid impacting write performance during warming.

    **Strategy 3: Cache persistence (Redis-specific)**:
    - Configure Redis RDB snapshots or AOF persistence so that cache data survives Redis restarts. After restart, Redis reloads data from disk — the cache is immediately warm.
    - **RDB**: Periodic snapshots (e.g., every 5 minutes). Fast recovery, but data written since the last snapshot is lost.
    - **AOF**: Every write operation is logged. Complete data recovery but slower restart (must replay the log). Use `AOF rewrite` to compact the log periodically.
    - **Both**: Use RDB for fast restart + AOF for completeness. Recommended for caches where warm restart is important.
    - Tradeoff: Persistence adds I/O overhead and disk space usage. For pure caches (data can be reloaded from the source), persistence is optional. For caches that are also used for sessions, rate limiting, or locks, persistence is important.

    **Strategy 4: Priming from a sibling instance**:
    - When a new application instance starts, it can copy the hot set from an already-warm sibling instance's L1 cache, or from the shared L2 cache. This is applicable in L1/L2 architectures.

    State the warming strategy and estimate the warming time and data source load impact.
