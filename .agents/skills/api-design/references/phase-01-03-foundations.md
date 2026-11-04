# API Foundations Reference (Phases 1-3)

Phases 1-3 establish the consumer context, choose the API style, and model the resources and URLs. Steps 1-9.

## Table of Contents
1. Phase 1: Consumer and Context Discovery (steps 1-5)
2. Phase 2: API Style Selection (step 6)
3. Phase 3: Resource Modeling and URL Design (steps 7-9)

---

## Phase 1: Consumer and Context Discovery

1. **Identify the API consumers.** Before designing any endpoint, establish who or what will consume this API. Ask directly if unclear: "Who are the consumers of this API — web frontends, mobile apps, third-party developers, internal microservices, or automated systems?" Different consumers drive fundamentally different design decisions. Produce an explicit consumer list:
   - **Consumer name/type** (e.g., "React SPA," "iOS app," "partner integration," "order-service").
   - **Trust level**: First-party (your own clients), second-party (trusted partners), third-party (public developers).
   - **Technical sophistication**: Are consumers experienced API integrators or teams that need maximum simplicity?
   - **Network context**: Are consumers on the public internet (latency-sensitive, untrusted) or on an internal network (low-latency, trusted)?
   - **Usage patterns**: Real-time interactive, batch processing, event-driven, or webhook-based?

2. **Extract the API's purpose and scope.** State in one to two sentences what this API exists to do. Example: "This API enables partner merchants to manage their product catalog, submit orders on behalf of customers, and retrieve fulfillment status." If the scope is unclear, force clarity before proceeding — an API without a well-defined purpose produces an incoherent design.

3. **Gather functional requirements as API-relevant use cases.** Translate product or system requirements into concrete API operations. For each use case, identify:
   - What action does the consumer want to perform?
   - What data does the consumer send?
   - What data does the consumer need back?
   - What are the preconditions and side effects?
   - Is this synchronous (consumer waits for result) or asynchronous (consumer is notified later)?

   Produce a numbered use case list. Example:
   > 1. Consumer creates a new order by submitting line items and shipping address → receives order ID and confirmation.
   > 2. Consumer retrieves the current status of an order by order ID → receives status, timestamps, and tracking info.
   > 3. Consumer cancels an order if it has not shipped → receives confirmation or rejection with reason.

4. **Extract non-functional API requirements.** Establish concrete targets for:
   - **Latency expectations**: What response time do consumers expect? (e.g., < 200ms p95 for reads, < 500ms p95 for writes.)
   - **Throughput**: Expected requests per second per consumer and in aggregate.
   - **Payload size constraints**: Are consumers on bandwidth-limited networks (mobile)? Are there large data transfers (file uploads, bulk exports)?
   - **Availability**: What SLA must this API meet? (e.g., 99.95% uptime.)
   - **Backward compatibility requirements**: How strictly must the API maintain backward compatibility? Are consumers able to update quickly, or do you need to support old versions for years?
   - **Compliance and data sensitivity**: Does the API handle PII, financial data, or health data that imposes constraints on what can be exposed, logged, or cached?

5. **Identify the API's relationship to the backend architecture.** Determine:
   - Is this a BFF (Backend for Frontend) API tailored to a specific client?
   - Is this a general-purpose platform API serving multiple consumers?
   - Is this a service-to-service internal API?
   - Is this a public developer API that must be self-service and self-documenting?
   - Does an API gateway sit in front of this API, or does the service handle cross-cutting concerns itself?

   This classification directly affects verbosity of responses, authentication design, versioning strategy, and documentation investment.

---

## Phase 2: API Style Selection

6. **Select the API style and justify the choice.** Make an explicit recommendation based on the consumer and context analysis:

   - **REST (HTTP/JSON)**: Recommend as the default for most APIs. Best when: the domain maps naturally to resources (nouns), consumers are diverse (browsers, mobile, third parties), cacheability matters, and you want maximum ecosystem compatibility (tooling, documentation, developer familiarity). REST is not a fallback — it is a deliberate choice for resource-oriented domains.

   - **GraphQL**: Recommend when: consumers have highly variable data-fetching needs across interconnected resources, over-fetching and under-fetching are measurable problems, and the API team can invest in query complexity management, depth limiting, and persisted queries. Explicitly state the costs: harder to cache at the HTTP level, requires complexity limiting to prevent abuse, N+1 query risk on the server, and steeper learning curve for some consumers. Do NOT recommend GraphQL solely because "it's flexible" — quantify the flexibility benefit.

   - **gRPC (Protocol Buffers)**: Recommend for: internal service-to-service communication where latency and type safety matter, high-throughput streaming use cases, and polyglot environments where code generation from proto definitions is valuable. Note the costs: poor browser support without gRPC-Web, less human-readable, requires protobuf tooling.

   - **WebSocket**: Recommend for: real-time bidirectional communication (chat, collaborative editing, live dashboards). Not appropriate for request-response patterns. Always pair with a REST or gRPC API for non-real-time operations.

   - **Server-Sent Events (SSE)**: Recommend for: server-to-client unidirectional streaming (live feeds, notification streams) when full WebSocket bidirectionality is unnecessary. Simpler than WebSocket, works over standard HTTP, auto-reconnects.

   - **Webhooks (callback-based async API)**: Recommend for: notifying external consumers of events asynchronously. Always pair with a polling fallback for consumers that cannot expose an endpoint.

   State the tradeoff explicitly: "For this system, I recommend REST because [specific reasons]. GraphQL was considered but rejected because [specific reasons]. If [specific condition] changes, reconsider GraphQL."

   If the system requires multiple styles (e.g., REST for CRUD + WebSocket for real-time), state that explicitly and define which operations use which style.

---

## Phase 3: Resource Modeling and URL Design (REST APIs)

7. **Model resources from the consumer's perspective, not the database schema.** This is the most critical step in REST API design. Resources are the API's conceptual model — they often aggregate, reshape, or simplify the underlying data model for consumer convenience.
   - List the primary resources (nouns) the API exposes. Example: `orders`, `products`, `customers`, `shipments`.
   - For each resource, define:
     - **What it represents** in one sentence.
     - **Its key attributes** (the fields consumers will see).
     - **Its relationships** to other resources (and how those relationships are represented — embedded, linked, or separate endpoint).
   - Distinguish between top-level resources (independently addressable) and sub-resources (only meaningful in the context of a parent). Example: `/orders/{orderId}/line-items` — line items are sub-resources of an order.
   - Design virtual or composite resources when the consumer's mental model doesn't map 1:1 to backend entities. Example: a `dashboard-summary` resource that aggregates data from multiple backend services.

8. **Design the URL structure.** Apply these rules consistently:
   - **Use plural nouns for collection endpoints**: `/orders`, `/products`, `/users`. Never use verbs in URLs (no `/getOrders`, `/createUser`).
   - **Use hierarchical paths for containment relationships**: `/customers/{customerId}/orders` — only when orders are truly scoped to a customer in this API's context.
   - **Limit nesting to two levels maximum**: `/customers/{customerId}/orders` is fine. `/customers/{customerId}/orders/{orderId}/line-items/{lineItemId}/discounts` is too deep — flatten it. Provide `/line-items/{lineItemId}` or `/orders/{orderId}/line-items` as alternatives.
   - **Use kebab-case for multi-word path segments**: `/order-items`, not `/orderItems` or `/order_items`.
   - **Use path parameters for identity**: `/orders/{orderId}`.
   - **Use query parameters for filtering, sorting, pagination, and optional modifiers**: `/orders?status=shipped&sort=-created_at&limit=20`.
   - **Resource IDs should be opaque**: Prefer UUIDs or encoded IDs over sequential integers to prevent enumeration and leaking of business data (e.g., total order count).

9. **Map HTTP methods to operations.** For each resource, define the supported operations using correct HTTP semantics:

   | Operation | Method | URL | Semantics |
   |---|---|---|---|
   | List/search | GET | `/orders` | Returns a collection. Must be safe and idempotent. |
   | Get one | GET | `/orders/{id}` | Returns a single resource. Must be safe and idempotent. |
   | Create | POST | `/orders` | Creates a new resource. Not idempotent by default (see step 20 for idempotency design). |
   | Full replace | PUT | `/orders/{id}` | Replaces the entire resource. Must be idempotent. Client sends the complete representation. |
   | Partial update | PATCH | `/orders/{id}` | Updates specific fields. Use JSON Merge Patch (RFC 7396) for simple cases or JSON Patch (RFC 6902) for complex operations. State which format and why. |
   | Delete | DELETE | `/orders/{id}` | Removes the resource. Must be idempotent (deleting an already-deleted resource returns 204 or 404 — decide and be consistent). |

   - **Do not use POST as a catch-all.** If an operation doesn't fit CRUD, model it as either: (a) a state transition on a resource (`PATCH /orders/{id}` with `{"status": "cancelled"}`), or (b) a sub-resource representing the action (`POST /orders/{id}/cancellation`). Choose the approach and apply it consistently. State the convention.
   - **RPC-style actions on resources**: When an operation is genuinely procedural (e.g., "recalculate pricing"), use `POST /orders/{id}/actions/recalculate` — but isolate this pattern and do not let it proliferate. If more than 20% of your endpoints are action-based, reconsider whether REST is the right style.