# Phase 12: HTTP Caching and CDN Design

24. **Design HTTP caching.** HTTP caching is the most efficient caching layer — it prevents requests from reaching your servers entirely. Design it using standard HTTP caching headers:

    **Cache-Control header design** — define per endpoint:
    - **Public, cacheable responses** (product catalog, public content):
      ```
      Cache-Control: public, max-age=300, s-maxage=600, stale-while-revalidate=60
      ```
      - `public`: Response can be cached by browsers, CDNs, and shared proxies.
      - `max-age=300`: Browser caches for 5 minutes.
      - `s-maxage=600`: CDN/shared proxies cache for 10 minutes (overrides `max-age` for shared caches).
      - `stale-while-revalidate=60`: After `max-age` expires, serve the stale response while revalidating in the background for up to 60 seconds. Eliminates latency for the first request after expiration.

    - **Private, user-specific responses** (user profile, order history):
      ```
      Cache-Control: private, max-age=60, no-transform
      ```
      - `private`: Only the end-user's browser can cache this. CDNs and shared proxies must not cache.
      - `max-age=60`: Browser caches for 1 minute.
      - `no-transform`: Proxies must not modify the response (no compression, no format conversion).

    - **Non-cacheable, sensitive responses** (authentication responses, financial data):
      ```
      Cache-Control: no-store
      ```
      - `no-store`: No caching at all — not by the browser, not by the CDN, not by any proxy. The response must be fetched from the server every time.
      - Do not use `no-cache` when you mean `no-store`. `no-cache` means "cache, but revalidate with the server before using" (it still stores the response). `no-store` means "do not store the response at all."

    - **Immutable assets** (versioned static files: `app.a1b2c3.js`):
      ```
      Cache-Control: public, max-age=31536000, immutable
      ```
      - `immutable`: The response body will never change for this URL. Browsers skip revalidation even on page refresh. Use only for content-addressed URLs (URL changes when content changes).

25. **Design conditional requests (ETags and Last-Modified).** For responses that change infrequently, conditional requests save bandwidth and server processing:

    **ETag-based** (recommended):
    - Server generates an ETag (a hash of the response body or a version identifier) and includes it in the response: `ETag: "a1b2c3d4"`.
    - On subsequent requests, the client sends `If-None-Match: "a1b2c3d4"`.
    - If the ETag matches (data has not changed), the server returns `304 Not Modified` with no body — saving bandwidth and serialization cost.
    - If the ETag does not match, the server returns the full response with the new ETag.
    - **Strong vs. weak ETags**: Strong ETags (`"a1b2c3"`) indicate byte-for-byte identical responses. Weak ETags (`W/"a1b2c3"`) indicate semantically equivalent responses (useful when the response format might vary slightly but the content is the same). Use strong ETags unless there is a specific reason for weak.
    - **ETag generation**: Hash the response body (SHA-256 or similar), or use a version counter/timestamp from the data source. Do not use the database row's `updated_at` timestamp as the sole ETag if the response aggregates data from multiple sources — use a composite hash.

    **Last-Modified-based** (simpler, less precise):
    - Server includes `Last-Modified: Wed, 15 Jan 2024 10:30:00 GMT`.
    - Client sends `If-Modified-Since: Wed, 15 Jan 2024 10:30:00 GMT`.
    - Server returns `304 Not Modified` or the full response.
    - Less precise than ETags (1-second granularity) and does not handle multiple representations of the same resource. Use ETags when possible.

26. **Design CDN caching strategy.** For public-facing APIs and content:

    **What to cache at the CDN**:
    - Static assets: Always. Long TTL + content-addressed URLs.
    - Public API responses: Product listings, content pages, public search results. Short to medium TTL (30s - 10min) based on update frequency.
    - Semi-personalized responses: Responses that vary by a small number of dimensions (locale, device type) can be cached at the CDN using `Vary` headers or cache key customization.

    **What NOT to cache at the CDN**:
    - Authenticated/user-specific API responses: Unless the CDN supports edge computing (Cloudflare Workers, Lambda@Edge, Fastly Compute) that can validate tokens and route to user-specific cache keys.
    - Write requests (POST, PUT, DELETE): CDNs should pass these through to the origin.
    - Real-time data: Stock prices, live sports scores, chat messages.

    **CDN cache key design**:
    - Default CDN cache key: URL + query parameters + `Vary` headers.
    - Customize the cache key at the CDN when needed:
      - Include specific headers (e.g., `Accept-Language`) to cache locale-specific responses.
      - Exclude irrelevant query parameters (tracking parameters like `utm_source`) from the cache key to increase hit rate.
      - Include the `Authorization` header presence (not the value) to separate authenticated vs. unauthenticated responses.
    - **`Vary` header**: Tells the CDN/browser which request headers cause response variation: `Vary: Accept-Language, Accept-Encoding`. Do not use `Vary: *` — it disables caching. Do not use `Vary: Cookie` — it creates a unique cache entry per user (effectively disabling caching for CDN). Use `Vary` sparingly and with known, bounded dimensions.

    **CDN cache invalidation**:
    - **API-based purge**: Purge specific URLs or patterns via the CDN provider's API when content changes: `purge("/api/v1/products/prod_abc")`.
    - **Surrogate keys (cache tags)**: Some CDNs (Fastly, Cloudflare) support tagging responses with surrogate keys. Purge by tag: `purge(tag: "product:prod_abc")` — invalidates all responses tagged with that key, regardless of URL.
    - **Soft purge / stale-while-revalidate**: Instead of hard purge (immediate removal), mark the entry as stale and let the CDN revalidate on the next request. Prevents stampede at the CDN edge.
    - **Purge propagation time**: CDN purges are not instant — propagation across all edge nodes takes seconds to minutes. Design for this delay. If instant invalidation is required, use short `s-maxage` values rather than relying on purges.
