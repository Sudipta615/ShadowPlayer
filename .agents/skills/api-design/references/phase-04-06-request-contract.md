# API Request Contract Reference (Phases 4-6)

Phases 4-6 shape the request/response contract, error handling, and collection behavior (pagination, filtering, sorting, search). Steps 10-20.

## Table of Contents
1. Phase 4: Request and Response Design (steps 10-13)
2. Phase 5: Error Handling and Validation Feedback (steps 14-16)
3. Phase 6: Pagination, Filtering, Sorting, and Search (steps 17-20)

---

## Phase 4: Request and Response Design

10. **Design a consistent response envelope.** Define a standard wrapper that every endpoint follows. Recommend one of:
    - **Envelope format** (recommended for public and partner APIs):
      ```json
      {
        "data": { ... },
        "meta": {
          "request_id": "abc-123",
          "timestamp": "2024-01-15T10:30:00Z"
        }
      }
      ```
      For collections:
      ```json
      {
        "data": [ ... ],
        "meta": {
          "request_id": "abc-123",
          "timestamp": "2024-01-15T10:30:00Z"
        },
        "pagination": {
          "next_cursor": "eyJpZCI6MTAwfQ==",
          "has_more": true
        }
      }
      ```
    - **No envelope** (acceptable for internal APIs where middleware handles metadata): Return the resource directly. But error responses must still use a structured format.

    State the chosen approach and enforce it across all endpoints. Inconsistency in response shapes is the #1 source of consumer frustration.

11. **Design resource representations.** For each resource:
    - **Define the full attribute set** with field names, types, nullability, and descriptions.
    - **Use snake_case for field names** (most common in JSON APIs; state if camelCase is chosen for JavaScript-heavy consumers and apply consistently).
    - **Use ISO 8601 for all dates and timestamps** with timezone: `"2024-01-15T10:30:00Z"`. Never use Unix timestamps in API responses — they are unreadable by humans debugging integrations.
    - **Use strings for monetary values** to avoid floating-point precision issues: `"amount": "49.99"`, with a separate `"currency": "USD"` field, or use minor units as integers (`"amount_cents": 4999`). State the convention and be consistent.
    - **Represent enums as lowercase strings**, not integers: `"status": "shipped"`, not `"status": 3`. Document all valid enum values.
    - **Represent relationships** using one of these strategies (pick one and be consistent):
      - **Embedded objects**: Include the related resource inline. Good for tightly coupled data that is almost always needed together. Beware of deep nesting.
      - **Foreign key reference**: Include `"customer_id": "cust_123"` and let the consumer fetch the customer separately. Good for loosely coupled relationships.
      - **Expandable references** (recommended for flexible APIs): Return the ID by default, allow consumers to request expansion: `GET /orders/{id}?expand=customer,line_items`. Define which fields are expandable and the maximum expansion depth.

12. **Design request payloads.** For create and update operations:
    - **Accept only the fields the consumer should control.** Never accept server-generated fields (ID, created_at, updated_at) in create/update requests.
    - **Distinguish between required and optional fields.** Document which fields are required for creation vs. which have defaults.
    - **For PATCH operations**, define the merge semantics: Does sending `null` clear a field? Does omitting a field leave it unchanged? Document this explicitly — ambiguity in PATCH semantics causes bugs.
    - **Validate inputs at the API boundary** and return field-level validation errors (see Phase 5).
    - **Use consistent field names** between request and response. If the resource representation uses `shipping_address`, the create request should also use `shipping_address`, not `address` or `ship_to`.

13. **Design sparse fieldsets (field selection).** For APIs where responses contain many fields and consumers often need only a subset:
    - Support a `fields` query parameter: `GET /orders/{id}?fields=id,status,total,created_at`.
    - Define the behavior: does the response include only the listed fields, or are certain fields (like `id`) always included?
    - This reduces payload size and improves performance for mobile and bandwidth-constrained consumers.

---

## Phase 5: Error Handling and Validation Feedback

14. **Design a structured error response format.** Define a consistent error schema used by every endpoint:
    ```json
    {
      "error": {
        "code": "VALIDATION_FAILED",
        "message": "The request body contains invalid fields.",
        "details": [
          {
            "field": "email",
            "code": "INVALID_FORMAT",
            "message": "Must be a valid email address."
          },
          {
            "field": "quantity",
            "code": "OUT_OF_RANGE",
            "message": "Must be between 1 and 1000."
          }
        ],
        "request_id": "req_abc123"
      }
    }
    ```
    - **`code`**: Machine-readable error code. Use a documented, stable set of error codes (e.g., `VALIDATION_FAILED`, `RESOURCE_NOT_FOUND`, `INSUFFICIENT_PERMISSIONS`, `RATE_LIMIT_EXCEEDED`, `INTERNAL_ERROR`). These must never change once published.
    - **`message`**: Human-readable explanation. May change without breaking consumers.
    - **`details`**: Array of field-level or sub-errors. Essential for validation errors.
    - **`request_id`**: Correlation ID for debugging. Always include.

15. **Map error types to HTTP status codes consistently.** Define and enforce the mapping:

    | Situation | Status Code | Error Code |
    |---|---|---|
    | Validation error (bad input) | 400 | `VALIDATION_FAILED` |
    | Missing or invalid authentication | 401 | `AUTHENTICATION_REQUIRED` |
    | Authenticated but not authorized | 403 | `INSUFFICIENT_PERMISSIONS` |
    | Resource not found | 404 | `RESOURCE_NOT_FOUND` |
    | Method not allowed | 405 | `METHOD_NOT_ALLOWED` |
    | Conflict (e.g., duplicate creation) | 409 | `CONFLICT` |
    | Request entity too large | 413 | `PAYLOAD_TOO_LARGE` |
    | Unsupported media type | 415 | `UNSUPPORTED_MEDIA_TYPE` |
    | Unprocessable entity (semantic error) | 422 | `UNPROCESSABLE_ENTITY` |
    | Rate limit exceeded | 429 | `RATE_LIMIT_EXCEEDED` |
    | Internal server error | 500 | `INTERNAL_ERROR` |
    | Service unavailable (dependency down) | 503 | `SERVICE_UNAVAILABLE` |

    - **Never return 200 with an error body.** Status codes must accurately reflect the outcome.
    - **Differentiate 400 vs. 422**: Use 400 for syntactically malformed requests (malformed JSON). Use 422 for syntactically valid but semantically invalid requests (valid JSON but business rule violation). State this convention and apply consistently.
    - **For 401 and 403**, never leak information about resource existence. If a user is not authorized to access a resource, return 403 (or 404 to hide existence — decide and document the policy).

16. **Design error responses for downstream failures.** When the API depends on another service or external system that fails:
    - Do not proxy raw upstream error details to the consumer. Translate them into your API's error format.
    - Return 502 (Bad Gateway) or 503 (Service Unavailable) with a consumer-friendly message and a `Retry-After` header when appropriate.
    - Log the upstream error details server-side with the request ID for debugging.

---

## Phase 6: Pagination, Filtering, Sorting, and Search

17. **Design pagination for all collection endpoints.** No collection endpoint should return unbounded results. Choose one pagination strategy:
    - **Cursor-based pagination** (recommended as default): Best for large, frequently changing datasets. Returns an opaque cursor that points to the next page.
      ```
      GET /orders?limit=20&cursor=eyJpZCI6MTAwfQ==
      ```
      Response includes:
      ```json
      {
        "data": [...],
        "pagination": {
          "next_cursor": "eyJpZCI6MTIwfQ==",
          "has_more": true
        }
      }
      ```
      Advantages: stable under concurrent inserts/deletes, performs well with indexed columns. Disadvantage: cannot jump to arbitrary pages.

    - **Offset-based pagination**: Use only for small, static datasets where consumers need page-number navigation (e.g., admin dashboards).
      ```
      GET /products?limit=20&offset=40
      ```
      Include total count in response for page calculation. State the performance warning: `OFFSET` in SQL degrades at high values.

    - **Keyset pagination**: A variant of cursor-based using explicit column values instead of opaque cursors. Useful when consumers need to understand the ordering: `GET /orders?limit=20&after_id=ord_100&sort=created_at`.

    Define the **default page size** and **maximum page size** for each collection (e.g., default 20, max 100). Reject requests exceeding the maximum with a 400 error.

18. **Design filtering.** For each collection endpoint, define the filterable fields:
    - Use query parameters with clear operators: `?status=shipped&created_after=2024-01-01&total_min=100`.
    - For complex filtering, define a convention: either LHS brackets (`?price[gte]=100&price[lte]=500`) or a structured filter parameter. Choose one convention and apply consistently.
    - Document every supported filter, its type, and valid values.
    - Unsupported filter parameters should be rejected with a 400 error (do not silently ignore them — this hides bugs).

19. **Design sorting.** Define the sorting convention:
    - Recommend: `?sort=created_at` for ascending, `?sort=-created_at` for descending (prefix with `-`).
    - For multi-field sorting: `?sort=-created_at,name`.
    - Document which fields are sortable. Reject unsupported sort fields with a 400 error.
    - Every sortable field must have a database index that supports the sort efficiently.

20. **Design search.** If the API supports search:
    - Simple text search: `?q=search+terms` on a defined set of searchable fields.
    - Advanced search: Consider a dedicated `POST /orders/search` endpoint with a structured query body when search parameters are complex. This is an acceptable use of POST for a non-mutating operation because query complexity may exceed URL length limits.
    - Define what "search" means: full-text search, prefix match, fuzzy match? Document the behavior.