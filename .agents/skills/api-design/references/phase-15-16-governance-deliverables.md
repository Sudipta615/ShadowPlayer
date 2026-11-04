# API Governance & Deliverables Reference (Phases 15-16)

Phases 15-16 cover the design review checklist, governance standards, and the final deliverables of an API design engagement. Steps 42-44.

## Table of Contents
1. Phase 15: API Design Review and Governance (steps 42-43)
2. Phase 16: Architecture Output and Deliverables (step 44)

---

## Phase 15: API Design Review and Governance

42. **Define an API design review checklist.** Before any API is approved for implementation, verify:
    - [ ] Resource naming follows conventions (plural nouns, kebab-case).
    - [ ] HTTP methods are semantically correct.
    - [ ] Status codes are correctly mapped to outcomes.
    - [ ] Request and response schemas are fully defined with types, examples, and constraints.
    - [ ] Error responses follow the standard error format with machine-readable codes.
    - [ ] Pagination is implemented for all collection endpoints.
    - [ ] Authentication and authorization are defined for every endpoint.
    - [ ] Idempotency is designed for all non-idempotent write operations.
    - [ ] Rate limiting is defined.
    - [ ] No breaking changes are introduced to existing versions.
    - [ ] Sensitive data is not exposed in URLs, logs, or response bodies.
    - [ ] The OpenAPI spec is complete and validates without errors.
    - [ ] Consumer impact has been assessed — existing consumers are not broken.

43. **Define API governance standards.** For organizations with multiple APIs:
    - Maintain a central API style guide documenting all conventions (naming, pagination, error format, authentication, versioning). Every API team follows this guide.
    - Use automated linting (Spectral, Optic) in CI to enforce style guide rules on OpenAPI specs.
    - Maintain an API catalog / registry where all APIs are discoverable with their specs, owners, and status.
    - Define the API lifecycle stages: Draft → Review → Published → Deprecated → Sunset.
    - Assign API ownership: every API has a named team responsible for its design, reliability, and evolution.

---

## Phase 16: Architecture Output and Deliverables

44. **Produce API design deliverables.** At the conclusion of every API design engagement, produce:
    - **API design summary**: A concise document (1-2 pages) stating the API's purpose, consumers, authentication model, resource model, and key design decisions.
    - **Resource and endpoint inventory**: A table listing every resource, its endpoints, methods, and brief descriptions.
    - **OpenAPI / AsyncAPI specification**: Complete, valid, and ready for implementation or review.
    - **Example requests and responses**: For every endpoint, at least one success example and one error example with realistic data.
    - **Data model mapping**: How API resources map to backend entities/services — which service owns each resource, and how data is aggregated if a resource spans multiple services.
    - **Design decisions log**: A list of significant design decisions made during the engagement, with rationale and alternatives considered. Use ADR format (Title, Context, Decision, Consequences) for non-trivial decisions.
    - **Open questions**: Anything that requires stakeholder input, consumer feedback, or further investigation before implementation.