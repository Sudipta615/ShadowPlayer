# Security Audit Playbook

Use this reference for a user-requested bug hunt, a pre-launch review, or a focused audit of a risky area. The goal is to find **exploitable** issues and propose fixes, not to produce a generic checklist.

## 1. Preconditions and Safety

Before testing anything:

- Confirm ownership or written permission. If the user has not made that clear, ask.
- Agree on scope: repositories, environments, endpoints, roles, and what is out of scope.
- Prefer local or staging. Do not test production without explicit approval.
- Do not run destructive payloads, denial-of-service tests, load tests, or data exfiltration.
- Do not use real customer data. Use synthetic accounts and records.
- Prove a finding with the minimum safe evidence: a code path, a request that returns another user's record, or a failing test.
- If you find a live secret, do not print it. Record the location and type, and recommend immediate rotation.
- If a finding could expose real data, stop and report it before going further.

## 2. Reconnaissance

Build a map before reading individual handlers:

- **Entry points:** routes, server actions, API endpoints, webhooks, cron jobs, queue consumers, CLI commands.
- **Auth boundaries:** middleware, guards, decorators, session checks, API-key checks.
- **Roles and tenants:** role definitions, permission checks, tenant scoping, admin impersonation.
- **Data:** models, migrations, raw queries, serializers, exports, backups.
- **Rendering:** templates, client components, markdown/HTML rendering, rich-text editors.
- **Files and URLs:** uploads, downloads, parsers, image processing, outbound fetches, redirects.
- **Secrets and config:** env files, CI variables, client-exposed prefixes, feature flags, debug settings.
- **Dependencies:** lockfiles, install scripts, postinstall hooks, package provenance.
- **Observability:** logs, error handlers, alerting, audit trails.

For each entry point, ask: who can reach it, what do they control, what does it trust, and what does it do with that trust?

## 3. Audit Domains

### 3.1 Authentication and Sessions

**Look for:** custom password hashing, missing rate limits, session fixation, tokens that never expire, logout that does not invalidate, reset links that are reusable or predictable, MFA that can be skipped, OAuth flows missing `state`/`nonce`/PKCE, account enumeration.

**Safe proof:** repeated failed logins hit a limit; an old session stops working after logout; a reset token cannot be reused; login responses do not differ for existing and missing accounts.

**AI-code failure:** a polished login form with no rate limit, no session rotation, and a reset token stored forever.

### 3.2 Authorization, IDOR/BOLA, and Tenancy

**Look for:** handlers that fetch by object ID without owner/tenant scope, role checks that exist only in the UI, admin routes protected by the same guard as user routes, mass assignment of `role`/`tenantId`, tenant filters applied after the query, exports that ignore scope.

**Safe proof:** create two users or tenants; user B requests user A's object and receives 404/403, not the record. Test list, read, update, delete, export, and nested resources.

**AI-code failure:** `findById(id)` with no `where userId` or `where tenantId`; a hidden admin button with an unprotected API.

### 3.3 Injection

**Look for:** string-built SQL, ORM raw fragments, NoSQL operators from user input, shell commands with interpolation, template engines rendering untrusted input, LDAP/XPath queries, unsafe `eval`/`Function`, header injection.

**Safe proof:** a benign payload such as `' OR 1=1 --` changes the query result or is rejected; a schema rejects operator objects; a shell argument is passed as an array, not a string.

**AI-code failure:** a parameterized query in one handler and a raw `WHERE name = '${name}'` in another.

### 3.4 XSS, CSRF, and CORS

**Look for:** `dangerouslySetInnerHTML`, `innerHTML`, unescaped template output, markdown rendering with raw HTML, user-controlled URLs in `href`/`src`, missing CSRF protection on cookie-auth writes, CORS `*` combined with credentials, permissive `Access-Control-Allow-Origin` reflection.

**Safe proof:** a harmless `<img src=x onerror=alert(1)>` is escaped or sanitized; a cross-origin write without a token is rejected; the browser preflight rejects an unlisted origin.

**AI-code failure:** sanitizing on the client but rendering raw HTML on the server; CORS reflecting any origin because it “just works” in development.

### 3.5 SSRF, Path Traversal, and File Uploads

**Look for:** user-controlled URLs passed to `fetch`/HTTP clients, image or PDF fetchers, webhook testers, redirects, archive extraction, file paths built from input, uploads that trust MIME type, files stored under a public/executable path, missing size limits.

**Safe proof:** a URL pointing at a private/link-local/metadata address is rejected; `../` cannot escape the allowed root; an executable file disguised with an image MIME type is rejected; oversized uploads are refused.

**AI-code failure:** “fetch this image from a URL” with no destination allowlist; uploads written to `public/uploads` with the original filename.

### 3.6 Secrets and Configuration

**Look for:** keys in source or Git history, `NEXT_PUBLIC_`/`VITE_` variables holding server secrets, secrets in logs or error responses, debug mode in production, default credentials, public databases, permissive storage buckets, test accounts left active.

**Safe proof:** search the working tree and history; inspect the built client bundle; confirm production env values are not exposed; confirm debug endpoints return 404.

**AI-code failure:** a `.env` committed “for convenience”; a service-role key used in a client component.

### 3.7 Cryptography and Data Protection

**Look for:** MD5/SHA-1 for passwords, static IVs, ECB mode, homemade crypto, secrets encrypted with keys stored beside them, missing TLS verification, sensitive data in URLs or logs, over-fetching PII, backups without access control.

**Safe proof:** inspect the algorithm and mode; confirm TLS verification is on; confirm sensitive fields are not returned by default; confirm backups are encrypted and access-controlled.

**AI-code failure:** `crypto.createHash('sha256')` used for passwords; “encryption” that is actually base64 encoding.

### 3.8 Abuse, Rate Limits, and Business Logic

**Look for:** unlimited signup, credential stuffing, OTP brute force, coupon/discount abuse, negative quantities, refund duplication, race conditions, replay of one-time actions, enumeration through search/export, expensive queries without limits.

**Safe proof:** repeat the action and observe the limit; submit concurrent requests and check for double-spend; replay a token and confirm it fails the second time.

**AI-code failure:** rate limiting only on `/login`; a refund endpoint that trusts a client-supplied amount; no idempotency on payment actions.

### 3.9 Dependencies and Supply Chain

**Look for:** known vulnerable versions, unmaintained packages, typosquatted names, install scripts, unpinned versions, lockfile drift, unnecessary permissions in SDKs, CI credentials with broad access.

**Safe proof:** run the ecosystem audit tool; inspect the lockfile and install scripts; verify package names and provenance; confirm CI tokens are scoped.

**AI-code failure:** installing a random package suggested by a model without checking ownership, downloads, or maintenance.

### 3.10 Logging, Errors, and Monitoring

**Look for:** stack traces in production, SQL or internal paths in responses, passwords/tokens/PII in logs, missing auth-failure logs, no alerting, logs that cannot be correlated, no revocation path.

**Safe proof:** trigger a handled error and inspect the response; search logs for sensitive fields; confirm a failed authorization produces an audit event; confirm alerts fire.

**AI-code failure:** `console.log(req.body)` on a login route; a generic “something went wrong” with no server-side context.

## 4. Severity Rubric

| Severity | Criteria | Examples |
|---|---|---|
| **Critical** | Pre-auth takeover, remote code execution, full cross-tenant or full-dataset breach, direct payment manipulation | Auth bypass, SQL injection dumping all users, unrestricted admin impersonation |
| **High** | Authenticated privilege escalation, cross-user data access, live secret exposure, SSRF to internal services, stored XSS in an admin surface | IDOR on invoices, role change via mass assignment, service key in the client bundle |
| **Medium** | Limited data exposure, CSRF on a sensitive action, missing auth rate limit, verbose production errors, weak session policy | Password reset without rate limit, stack traces with internal paths |
| **Low** | Hardening gap, minor information leak, non-exploitable outdated dependency, missing best-practice header | Missing `Referrer-Policy`, outdated dev-only package |

Add **confidence**:

- **Confirmed:** reproduced or proven by code path and test.
- **Likely:** strong evidence, but not fully reproduced.
- **Needs validation:** plausible, requires environment access or product knowledge.

Do not inflate severity. A missing header is not Critical because it sounds scary. A confirmed cross-tenant read is not Medium because the app is small.

## 5. Finding Template

```markdown
### [HIGH] Cross-user invoice access via invoiceId
- ID: SEC-001
- Confidence: Confirmed
- Location: app/api/invoices/[id]/route.ts:24
- Evidence: query filters by invoiceId only; no tenantId or userId scope
- Exploit path: authenticated user changes invoiceId in GET /api/invoices/:id
- Impact: reads another customer's invoice, billing address, and line items
- Fix: scope the query by tenantId and userId; return 404 when no match
- Verification: two-user test; user B receives 404 for user A's invoice
- Class: OWASP A01 Broken Access Control / CWE-639
```

Rules:

- One finding per issue. Do not merge unrelated problems into one entry.
- Put the most severe findings first.
- Include file/line, route, or code path; the report must be actionable.
- Redact secrets, tokens, and personal data from evidence.
- Distinguish **vulnerability** from **hardening**. Hardening has no current exploit path.
- If a finding depends on a precondition (admin account, feature flag, specific tenant), state it.

## 6. Remediation Plan

End with a plan the user can execute:

| Priority | Finding | Fix | Effort | Verification |
|---|---|---|---|---|
| Fix now | SEC-001 | Scope invoice query by tenant | S | Two-user test |
| Before release | SEC-004 | Add reset-token rate limit | S | Repeated-reset test |
| Backlog | SEC-007 | Add `Referrer-Policy` | XS | Header check |

Priorities:

- **Fix now:** Critical or High, exploitable in the current build. Block release.
- **Before release:** realistic High/Medium path. Fix in the current cycle.
- **Backlog:** Low or hardening. Assign an owner and date.
- **Accept:** risk understood and accepted. Record rationale, owner, and review trigger.

For each fix, add a regression test. A fix without a test will reopen.

## 7. Reporting Rules

- **Findings first, summary second.** Do not bury a Critical issue under a long methodology section.
- **State coverage and limits.** “Reviewed auth, authz, uploads, and dependency config; did not review infrastructure or mobile clients.”
- **No secrets or PII** in the report, logs, or chat.
- **No “100% secure”.** State what was tested and what was not.
- **Group low-value noise.** Ten missing headers can be one hardening item.
- **Do not run exploits against production** to prove a point.
- **Recommend fixes, not fear.** Every finding needs a path forward.
- **If nothing is found,** say what was covered and what remains unverified. Absence of findings is not proof of security.

## 8. Common AI-Code Failure Patterns

| Pattern | Audit check |
|---|---|
| Frontend-only authorization | Every server handler independently checks permission |
| Hardcoded secrets | Working tree, Git history, built client bundle |
| Broken tenant isolation | Every read/write query includes tenant scope |
| Blind trust in generated code | Security tests for authz, injection, abuse cases |
| Unsafe dependencies | Audit, lockfile, package provenance, install scripts |
| Insecure production config | Debug off, CORS allowlist, private DB, no default creds |
| Unsafe uploads | Type/size/content checks, isolated storage, no executable path |
| No regression tests | A test exists for every previously fixed finding |
| No abuse-case tests | Unauthorized, malformed, race, replay, and limit tests |
| No detection or recovery | Auth-failure logs, alerts, key rotation, tested restore |

## 9. Audit Checklist

- [ ] Permission and scope confirmed.
- [ ] Attack surface mapped before reading handlers.
- [ ] Auth and session behavior tested.
- [ ] Cross-user and cross-tenant access tested on read, list, update, delete, and export.
- [ ] Injection paths reviewed for SQL, NoSQL, command, template, and LDAP.
- [ ] XSS, CSRF, and CORS reviewed at the server boundary.
- [ ] SSRF, path traversal, and upload handling reviewed.
- [ ] Secrets checked in source, history, logs, errors, and client bundle.
- [ ] Crypto and sensitive-data exposure reviewed.
- [ ] Abuse, rate limits, race conditions, and business logic tested.
- [ ] Dependencies and supply chain checked.
- [ ] Logging, errors, and monitoring reviewed.
- [ ] Every finding has severity, confidence, evidence, exploit path, impact, fix, and verification.
- [ ] Remediation plan ordered by exploitability × impact.
- [ ] Coverage and limitations stated.
- [ ] No destructive test or real data exposure occurred.
