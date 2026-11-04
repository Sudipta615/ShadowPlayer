# Phase 6: Cache Consistency Management

14. **Design for consistency explicitly.** Every cached data type has a consistency requirement. Document it:

    | Cached Data | Consistency Model | Max Staleness | Consequence of Stale Data | Invalidation Strategy |
    |---|---|---|---|---|
    | Product catalog | Eventual | 5 minutes | User sees old price/description | TTL 5min + event invalidation |
    | User profile (own) | Read-your-own-write | 0 seconds | User doesn't see their own edits | Invalidate on write + bypass cache for writer |
    | User profile (others) | Eventual | 1 minute | Slight delay in seeing profile changes | TTL 1min |
    | Inventory count | Strong (or none) | 0 seconds | Overselling | Do not cache, or cache with TTL 5s + pessimistic stock check on order |
    | Feature flags | Eventual | 30 seconds | Feature toggle takes 30s to propagate | TTL 30s + event invalidation |
    | Search results | Eventual | 10 minutes | New/updated products don't appear immediately | TTL 10min |
    | Dashboard aggregates | Eventual | 5 minutes | Dashboards slightly behind real-time | TTL 5min, refresh-ahead |

    **Read-your-own-write consistency**: After a user makes a change, that user must see their own change immediately, even if other users see a stale version. Implementation:
    - On write: invalidate or update the cache entry.
    - For the writing user's subsequent reads: either bypass the cache for a short window (set a per-user `last_write_timestamp` and skip cache if `now - last_write_timestamp < threshold`), or read from the primary database rather than a cache/replica.
    - Other users' reads: continue serving from cache with normal TTL.

    **Cross-service cache consistency**: When a cache in Service A depends on data owned by Service B:
    - Service B publishes change events → Service A subscribes and invalidates its cache.
    - If the event system fails, Service A's TTL eventually expires and the cache is refreshed.
    - The staleness window is: min(TTL, event_delivery_latency). Design both to meet the consistency requirement.
