# Authorization and Access Control

This reference covers **Phase 4: Authorization and Access Control**. See the main SKILL.md for the phase summary and link.

---

9. **Design the authorization architecture.** Authorization (what an authenticated entity is allowed to do) must be designed as a separate, explicit system — not scattered across application code:

   **Select the authorization model**:
   - **RBAC (Role-Based Access Control)**: Users are assigned roles, roles have permissions. Simplest model. Recommended when: the permission structure is relatively flat, roles are few and well-defined, and most users within a role have identical access.
     - Define roles: `admin`, `editor`, `viewer`, `billing_admin`, etc.
     - Define permissions: `orders:read`, `orders:write`, `orders:delete`, `users:manage`, etc.
     - Map roles to permission sets. A user may have multiple roles.
     - Evaluate on every request: "Does this user's role(s) include the required permission for this action?"
   - **ABAC (Attribute-Based Access Control)**: Access decisions based on attributes of the user, the resource, the action, and the environment. More flexible than RBAC. Recommended when: access rules depend on resource ownership ("user can edit only their own orders"), organizational hierarchy ("manager can view their team's reports"), data classification ("only users with clearance level X can view restricted documents"), or contextual factors ("access only from the corporate network during business hours").
     - Define policies as rules: `ALLOW if user.department == resource.department AND action == "read"`.
     - Use a policy engine (OPA/Rego, Cedar, Casbin) to evaluate policies consistently.
   - **ReBAC (Relationship-Based Access Control)**: Access decisions based on the relationship between the user and the resource, modeled as a graph. Recommended when: permissions are inherently relational ("user can access this document because they are a member of the team that owns the folder that contains it"), and relationships are complex and deeply nested. Implementations: Zanzibar (Google), SpiceDB, Ory Keto, AuthZed.

   **Authorization enforcement points**:
   - **API gateway / middleware**: Coarse-grained checks (is the user authenticated? does their token contain the required scope?). Reject obviously unauthorized requests early.
   - **Service layer** (primary enforcement): Fine-grained checks (does this user have permission to perform this action on this specific resource?). Every business operation must check authorization before execution. Never rely solely on the API layer — internal callers and future API changes may bypass it.
   - **Data layer** (defense in depth): Row-Level Security (RLS) in the database for multi-tenant systems. Even if the application code has a bug that omits a tenant filter, the database rejects cross-tenant queries.
   - **UI layer**: Hide or disable UI elements the user cannot access, but never rely on this as a security control — it is purely a UX enhancement. All authorization enforcement must happen server-side.

10. **Design authorization for common patterns:**

    **Resource-level authorization (preventing IDOR)**:
    - Every endpoint that accesses a specific resource must verify that the authenticated user is authorized to access that specific resource. `GET /orders/{orderId}` must verify the order belongs to the requesting user (or the user has admin access).
    - Never assume that because a user is authenticated, they can access any resource of the type they are authorized to use. Authentication proves identity; authorization proves access to the specific resource.
    - Implementation: After fetching the resource, verify ownership or permission before returning data. Alternatively, include the user/tenant scope in the database query itself: `WHERE id = $1 AND tenant_id = $2`.

    **Horizontal privilege escalation prevention**:
    - A regular user should not be able to access another regular user's resources by guessing or enumerating IDs.
    - Use opaque identifiers (UUIDs) to make enumeration harder (but this is not a substitute for access control checks — security through obscurity is not a control).
    - Always validate resource ownership server-side.

    **Vertical privilege escalation prevention**:
    - A regular user should not be able to perform admin actions by calling admin endpoints directly or by manipulating request parameters (e.g., setting `{"role": "admin"}` in a profile update).
    - Admin endpoints must have explicit role/permission checks.
    - Use separate API routes or middleware for admin functions when possible.

    **Function-level access control**:
    - Every API endpoint must have a defined authorization requirement, even if it is "authenticated user" (not "anyone"). Review all endpoints for missing authorization checks — endpoints added during rapid development often lack them.
    - Maintain an authorization matrix: a table listing every endpoint, the required permission, and the roles that have that permission. Review this matrix periodically.

11. **Design authorization for multi-tenancy.**
    - **Tenant context injection**: On every request, resolve the tenant from the authentication token (tenant_id claim in JWT, organization context in the session). Never rely on the client to specify the tenant in the request body or URL — the server must determine it from the authenticated identity.
    - **Tenant scoping in queries**: Every database query must include the tenant scope. Centralize this in a query middleware, ORM scope, or database Row-Level Security policy.
    - **Cross-tenant access prevention testing**: Write specific tests that attempt to access Tenant B's data using Tenant A's credentials. These tests must fail. Run them in CI on every build.
    - **Tenant admin vs. system admin**: Distinguish between a tenant's administrator (can manage users and data within their tenant) and a system administrator (can manage the platform across all tenants). Use separate roles and separate access paths.