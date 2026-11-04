# API Docs, Testing & Operations Reference (Phases 12-14)

Phases 12-14 cover documentation/developer experience, the testing strategy, and API gateway / cross-cutting concerns. Steps 35-41.

## Table of Contents
1. Phase 12: API Documentation and Developer Experience (steps 35-37)
2. Phase 13: API Testing Strategy (steps 38-39)
3. Phase 14: API Gateway and Cross-Cutting Concerns (steps 40-41)

---

## Phase 12: API Documentation and Developer Experience

35. **Produce an API specification.** For every API designed:
    - **REST APIs**: Produce an OpenAPI 3.x specification. This is the source of truth. Define:
      - Every endpoint with method, path, summary, description.
      - Request parameters (path, query, header) with types, required/optional, descriptions, and examples.
      - Request body schemas with field-level descriptions, types, constraints (minLength, maxLength, pattern, enum), and examples.
      - Response schemas for every status code the endpoint can return — success and every error case.
      - Authentication requirements per endpoint.
      - Reusable schema components for shared models.
    - **GraphQL APIs**: Produce the schema definition with descriptions on every type, field, and argument. Include example queries and mutations.
    - **gRPC APIs**: Produce `.proto` files with comments on every service, RPC, and message.
    - **Async APIs (webhooks, events)**: Produce an AsyncAPI 2.x specification defining event types, payload schemas, and delivery semantics.

36. **Design the API documentation structure.** Beyond the specification, the API must have:
    - **Getting started guide**: Authentication setup, first API call example, and a working curl/code snippet that a developer can copy-paste and run within 5 minutes.
    - **Authentication guide**: Step-by-step instructions for obtaining and using credentials for each auth method.
    - **Core concepts**: Explain the resource model, relationships, conventions (naming, pagination, error format) before diving into endpoint reference.
    - **Endpoint reference**: Auto-generated from the OpenAPI spec. Include request and response examples for every endpoint — not just schemas, but complete, realistic examples.
    - **Error reference**: Document every error code, what causes it, and how to resolve it.
    - **Rate limiting documentation**: Limits, headers, and retry guidance.
    - **Changelog**: Every change to the API, with dates, categorized as additions, deprecations, or breaking changes.
    - **Migration guides**: Per version bump, a step-by-step migration guide.
    - **SDKs and code examples**: If applicable, provide client libraries or at minimum, code examples in the top 2-3 languages used by consumers.

37. **Design spec-first vs. code-first workflow.** Make an explicit recommendation:
    - **Spec-first** (recommended for public and partner APIs): Write the OpenAPI spec first, review it with consumers, then generate server stubs and client SDKs. Ensures the API is designed for consumers, not shaped by implementation convenience.
    - **Code-first** (acceptable for internal APIs): Annotate code to generate the OpenAPI spec. Faster iteration, but requires discipline to keep the generated spec consumer-friendly.
    - Regardless of approach, the spec must be versioned in source control alongside the code and validated in CI (linting with tools like Spectral, breaking change detection with tools like optic or openapi-diff).

---

## Phase 13: API Testing Strategy

38. **Design the API testing pyramid.** Define testing layers:
    - **Unit tests**: Test individual request validation, business logic, and response serialization in isolation. Fast, run on every commit.
    - **Integration tests**: Test each endpoint against a real (or containerized) database and dependencies. Verify correct status codes, response shapes, error handling, pagination behavior, and authorization enforcement. Run in CI.
    - **Contract tests**: For service-to-service APIs, use consumer-driven contract testing (Pact or similar). The consumer defines the contract, the provider verifies it. Prevents unintentional breaking changes.
    - **End-to-end / smoke tests**: A small suite of critical-path tests run against staging and production after deployment. Verify authentication, core CRUD operations, and key business flows.
    - **Performance / load tests**: Run against a staging environment that mirrors production topology. Measure p50, p95, p99 latency under expected and peak load. Identify breaking points.
    - **Security tests**: Automated OWASP ZAP or similar scanning of API endpoints. Test authentication bypass, injection, broken authorization (IDOR), and rate limiting enforcement.

39. **Define the test data strategy.** Specify:
    - How test data is seeded for integration tests (fixtures, factories, or database snapshots).
    - How test data is isolated between test runs (transactional rollback, per-test database, or namespace isolation).
    - How production-like data is generated for performance tests without using actual production data (synthetic data generation).

---

## Phase 14: API Gateway and Cross-Cutting Concerns

40. **Design API gateway responsibilities.** If an API gateway is in the architecture, define what it handles:
    - **Routing**: Map external URLs to internal service endpoints. Define the routing rules.
    - **Authentication**: Validate tokens/API keys at the gateway to reject unauthenticated requests before they reach services.
    - **Rate limiting**: Enforce rate limits centrally.
    - **Request/response transformation**: Header injection (add trace IDs), response envelope wrapping, protocol translation (e.g., REST to gRPC).
    - **TLS termination**: Terminate HTTPS at the gateway, forward HTTP internally (or re-encrypt for zero-trust).
    - **Logging and metrics**: Capture access logs and request metrics for all API traffic in one place.
    - **CORS handling**: Manage CORS preflight responses centrally.

    Choose the gateway technology and justify: Kong, AWS API Gateway, Envoy/Ambassador, Apigee, or a lightweight custom gateway. Base the choice on feature needs, team expertise, and infrastructure context.

41. **Design request correlation and tracing.** Ensure every API request is traceable:
    - Generate a unique `X-Request-Id` (or use the consumer-provided one) at the API gateway.
    - Propagate the request ID through all downstream service calls, message queues, and log entries.
    - Return the `X-Request-Id` in every response (including error responses) so consumers can reference it in support requests.
    - Integrate with distributed tracing (OpenTelemetry) — the request ID should be the trace ID or correlated with it.