# Async & Performance Reference (Phases 10-11)

Phases 10-11 cover asynchronous API patterns (long-running operations, webhooks, streaming) and performance/caching. Steps 30-34.

## Table of Contents
1. Phase 10: Asynchronous API Patterns (steps 30-32)
2. Phase 11: API Performance and Caching (steps 33-34)

---

## Phase 10: Asynchronous API Patterns

30. **Design long-running operation APIs.** For operations that take longer than acceptable response times (> 5-10 seconds):
    - **Async request-response pattern**:
      1. Consumer sends `POST /exports` → Server returns `202 Accepted` with a status URL:
         ```json
         {
           "data": {
             "operation_id": "op_abc123",
             "status": "processing",
             "status_url": "/operations/op_abc123"
           }
         }
         ```
      2. Consumer polls `GET /operations/op_abc123` until status is `completed` or `failed`.
      3. When completed, the response includes the result or a link to download it.
    - Include `Retry-After` header in 202 responses to guide polling frequency.
    - For consumers that support it, offer webhook notification as an alternative to polling (see step 31).

31. **Design webhook APIs.** For event notification to external consumers:
    - **Registration**: `POST /webhooks` with `{ "url": "https://consumer.com/callback", "events": ["order.created", "order.shipped"], "secret": "..." }`.
    - **Delivery**: POST to the registered URL with a signed payload:
      ```json
      {
        "event_id": "evt_abc123",
        "event_type": "order.shipped",
        "timestamp": "2024-01-15T10:30:00Z",
        "data": { ... }
      }
      ```
    - **Signature verification**: Sign payloads using HMAC-SHA256 with the consumer's secret. Include the signature in a header (`X-Webhook-Signature`). Document the verification algorithm with code examples.
    - **Retry policy**: Retry failed deliveries with exponential backoff (e.g., 5s, 30s, 2m, 15m, 1h, 4h). Define the maximum retry count and what happens after exhaustion (disable the webhook, alert the consumer).
    - **Idempotency**: Include `event_id` in every delivery. Consumers must deduplicate by `event_id`.
    - **Ordering**: State that ordering is not guaranteed across events. If ordering matters, include a sequence number or timestamp that consumers can use to handle out-of-order delivery.
    - **Health monitoring**: Implement webhook health tracking. Disable webhooks that consistently fail and notify the consumer.

32. **Design streaming APIs (if applicable).** If the system requires real-time data streaming:
    - **Server-Sent Events (SSE)**: For server-to-client unidirectional streaming. Define the event types, reconnection behavior (`Last-Event-ID` header for resume), and keepalive interval.
    - **WebSocket**: For bidirectional streaming. Define the connection handshake, message format (JSON over WebSocket is recommended for simplicity), heartbeat/ping-pong interval, and reconnection strategy. Define the message types (commands, events, acknowledgments) as a schema.
    - **gRPC streaming**: For internal service-to-service streaming. Define server-streaming, client-streaming, or bidirectional-streaming RPCs as appropriate.

---

## Phase 11: API Performance and Caching

33. **Design HTTP caching.** For REST APIs, leverage HTTP caching semantics:
    - **Define cacheability per endpoint**:
      - Public, static resources (product catalog): `Cache-Control: public, max-age=300`.
      - User-specific resources (my orders): `Cache-Control: private, max-age=60`.
      - Sensitive or real-time data: `Cache-Control: no-store`.
    - **Use ETags for conditional requests**:
      - Response includes `ETag: "abc123"`.
      - Consumer sends `If-None-Match: "abc123"` on subsequent requests.
      - Server returns `304 Not Modified` if unchanged — saves bandwidth and processing.
    - **Use `Last-Modified` / `If-Modified-Since`** as an alternative or complement to ETags for time-based resources.
    - For collections that change frequently, consider shorter TTLs and rely on CDN or API gateway caching rather than client caching.

34. **Design API response optimization.** Address performance through API design:
    - **Compression**: Support `Accept-Encoding: gzip` and respond with `Content-Encoding: gzip` for all JSON responses. This typically reduces payload size by 60-80%.
    - **Field selection** (step 13): Reduce response size by allowing consumers to request only needed fields.
    - **Eager loading vs. lazy loading**: Define which related resources are included by default vs. requiring `expand` parameters. Default to minimal responses — let consumers opt into more data.
    - **Avoid N+1 API calls**: If consumers routinely need to call endpoint A to get IDs, then call endpoint B for each ID, design a batch endpoint or add expansion support to endpoint A.