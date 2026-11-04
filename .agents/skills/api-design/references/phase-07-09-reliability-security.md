# API Reliability & Security Reference (Phases 7-9)

Phases 7-9 cover idempotency and safe retries, authentication/authorization/security, and versioning/evolution. Steps 21-29.

## Table of Contents
1. Phase 7: Idempotency and Safe Retry Design (steps 21-22)
2. Phase 8: Authentication, Authorization, and API Security (steps 23-26)
3. Phase 9: Versioning and Evolution (steps 27-29)

---

## Phase 7: Idempotency and Safe Retry Design

21. **Design idempotency for write operations.** Every API that consumers will retry (due to network failures, timeouts, or uncertainty) must be idempotent:
    - **GET, PUT, DELETE** are inherently idempotent by HTTP semantics. Ensure your implementation honors this.
    - **POST (create operations)** are not inherently idempotent. Design idempotency using an `Idempotency-Key` header:
      ```
      POST /orders
      Idempotency-Key: client-generated-uuid-abc123
      ```
      Server behavior:
      - First request: Process normally, store the result keyed by the idempotency key.
      - Subsequent requests with the same key: Return the stored result without reprocessing.
      - Define the idempotency key TTL (e.g., 24 hours) — after which the key expires and the same key would be treated as a new request.
      - Define the storage mechanism for idempotency keys (Redis with TTL, or a database table with cleanup).
    - **For financial or critical operations**, idempotency is mandatory, not optional. Design it into the API contract from day one.

22. **Design bulk and batch operations.** When consumers need to operate on multiple resources in a single request:
    - **Batch create**: `POST /orders/batch` with an array body. Define the maximum batch size (e.g., 100 items). Return individual results for each item:
      ```json
      {
        "results": [
          { "index": 0, "status": "created", "data": { "id": "ord_1" } },
          { "index": 1, "status": "failed", "error": { "code": "VALIDATION_FAILED", "message": "..." } }
        ]
      }
      ```
    - Use HTTP 207 (Multi-Status) or 200 with per-item status — decide and be consistent.
    - **Define atomicity**: Does the batch succeed or fail as a whole (all-or-nothing), or are partial successes allowed? State explicitly. Partial success is usually the better consumer experience for batch operations.
    - **Batch operations must support idempotency** via the `Idempotency-Key` header.

---

## Phase 8: Authentication, Authorization, and API Security

23. **Design the authentication mechanism.** Select based on consumer type:
    - **OAuth 2.0 + OpenID Connect**: For user-facing APIs where consumers act on behalf of end users. Define the grant types:
      - Authorization Code with PKCE: For SPAs and mobile apps (always with PKCE, never implicit grant).
      - Client Credentials: For service-to-service communication where no user context is needed.
    - **API Keys**: For third-party developer APIs where simplicity matters. API keys identify the consumer application, not the end user. Always transmit in a header (`X-API-Key` or `Authorization: ApiKey <key>`), never in query parameters (they leak in logs and browser history).
    - **JWT Bearer Tokens**: For stateless authentication. Define: issuer, audience, expiry duration, claims included, and signature algorithm (RS256 for asymmetric, HS256 only for internal). Define token refresh strategy — short-lived access tokens (15 min) with longer-lived refresh tokens (7-30 days) with rotation.
    - **Mutual TLS (mTLS)**: For internal service-to-service communication in high-security environments.

    State which mechanism is used for each consumer type. Never use Basic Auth over non-TLS connections.

24. **Design the authorization model.** Define how permissions are enforced:
    - **At which layer**: API gateway (coarse-grained: "is this consumer allowed to call this endpoint?") and/or service layer (fine-grained: "is this user allowed to access this specific order?").
    - **Authorization patterns**:
      - RBAC (Role-Based Access Control): Simplest. Users have roles, roles have permissions. Sufficient for most systems.
      - ABAC (Attribute-Based Access Control): When access decisions depend on resource attributes (e.g., "user can only view orders from their own department"). More complex but more flexible.
      - Relationship-based (e.g., Zanzibar/SpiceDB): When authorization depends on complex object relationships (e.g., "user can edit this document because they are a member of the team that owns the folder containing it").
    - **Resource-level authorization**: Every endpoint that returns or modifies a specific resource must verify that the authenticated consumer has access to that specific resource. Never rely solely on endpoint-level authorization. This prevents IDOR (Insecure Direct Object Reference) vulnerabilities.
    - **Define scopes/permissions** that consumers request during OAuth: `orders:read`, `orders:write`, `products:admin`. Scopes should be granular enough to support least-privilege access.

25. **Design rate limiting and throttling.** Define the rate limiting strategy:
    - **Rate limit dimensions**: Per API key, per user, per endpoint, or per consumer tier. Recommend per-consumer with per-endpoint overrides for expensive operations.
    - **Define specific limits**: e.g., "Standard tier: 100 requests/minute. Premium tier: 1000 requests/minute. Bulk export endpoint: 10 requests/hour."
    - **Response headers** (always include):
      ```
      X-RateLimit-Limit: 100
      X-RateLimit-Remaining: 42
      X-RateLimit-Reset: 1705312200
      ```
    - **429 response** when limit is exceeded, with `Retry-After` header.
    - **Rate limiting algorithm**: Token bucket (recommended — allows short bursts) or sliding window. State the choice.
    - **Rate limiting placement**: At the API gateway for simplicity and consistency, with the option for service-level rate limiting for expensive business operations.

26. **Design API security controls.** Address:
    - **TLS everywhere**: All API traffic must be over HTTPS. No exceptions.
    - **Input validation**: Validate all inputs at the API boundary — types, lengths, formats, ranges, and allowed values. Reject invalid inputs with 400/422 errors. Never trust client input.
    - **Mass assignment protection**: Explicitly define which fields are writable for each endpoint. Ignore unexpected fields in request bodies (or reject them — choose and document the policy).
    - **Response filtering**: Never expose internal fields (database IDs if you use UUIDs, internal status flags, server implementation details) in API responses.
    - **CORS configuration**: For browser-consumed APIs, define allowed origins, methods, and headers. Never use `Access-Control-Allow-Origin: *` on authenticated APIs.
    - **Request size limits**: Define maximum request body size per endpoint. Enforce at the gateway.
    - **SQL injection and injection attacks**: Use parameterized queries. Never construct queries from raw API input.
    - **Sensitive data handling**: Define which fields are sensitive (passwords, tokens, SSN, credit card numbers). These must never appear in logs, error messages, or non-essential API responses.

---

## Phase 9: Versioning and Evolution

27. **Design the API versioning strategy.** Select one approach and apply consistently:
    - **URL path versioning** (recommended as default): `/v1/orders`, `/v2/orders`. Most explicit, easiest for consumers to understand and for routing infrastructure to handle. Use this unless there is a specific reason not to.
    - **Header-based versioning**: `Accept: application/vnd.myapi.v2+json` or custom header `API-Version: 2`. Use when you need to version individual resources independently or want cleaner URLs. More complex for consumers to implement.
    - **Query parameter versioning**: `?version=2`. Avoid — it makes caching harder and is easy to forget.

    Do not use content negotiation for versioning unless the API is already using HATEOAS and the consumers are sophisticated.

    State the chosen approach and the rationale explicitly.

28. **Define backward compatibility rules.** Establish and document what constitutes a breaking vs. non-breaking change:

    **Non-breaking (safe to deploy without new version)**:
    - Adding a new optional field to a response.
    - Adding a new endpoint.
    - Adding a new optional query parameter.
    - Adding a new enum value to a response field (if consumers are instructed to handle unknown enum values gracefully).
    - Adding a new optional field to a request body.

    **Breaking (requires a new version)**:
    - Removing or renaming a response field.
    - Removing or renaming an endpoint.
    - Changing a field's type.
    - Making a previously optional request field required.
    - Changing the semantic meaning of a field or status code.
    - Removing a supported enum value.
    - Changing the URL structure.

    State the contract: "Consumers should tolerate unknown fields in responses and unknown enum values. The API will never remove or rename existing fields within a version."

29. **Design the deprecation and sunset process.** Define the lifecycle:
    - **Deprecation announcement**: Minimum lead time (e.g., 6 months for public APIs, 3 months for partner APIs, 1 month for internal APIs). Announced via changelog, email, and `Deprecation` response header.
    - **Deprecation headers**: Include `Deprecation: true` and `Sunset: Sat, 01 Jun 2025 00:00:00 GMT` headers on deprecated endpoints.
    - **Migration guide**: Every version bump must include a consumer-facing migration guide documenting what changed, why, and exactly how to update.
    - **Usage monitoring**: Track usage of deprecated endpoints. Do not sunset until usage drops below a defined threshold or the sunset date is reached.
    - **Sunset execution**: After the sunset date, return 410 (Gone) with a message directing consumers to the new version.