---
name: security-baseline
description: Use as the default security entry point when starting, shipping, or reviewing an app, or when the user asks "is this secure?", "find security bugs", "harden this", or "what security do I need?". Classifies the app from L0 prototype to L5 high-assurance, applies the mandatory L1 floor before feature work, adds only the controls the app's level requires, audits for exploitable issues with severity, evidence, exploit path, and fix proposal, and raises concise security reminders when a change touches auth, permissions, payments, uploads, secrets, or external input. Routes deep threat modeling to security, code-level patterns to secure-code-guardian, and auth flows to authentication.
license: MIT
metadata:
  author: Emmraan
  version: 1.0.0
---

# Security Baseline

Most AI-built apps are not compromised because the agent chose a weak cipher. They are compromised because nobody decided what level of protection the app needed, so basic controls were skipped while advanced ones were discussed.

**Core principle: match the security floor to the app's actual risk, then verify it with evidence.**

Three rules follow from that:

1. **Level 1 is mandatory** for anything internet-facing and production-bound.
2. **Higher levels are additive, not automatic.** Add them when the app's data, exposure, or blast radius justifies the cost.
3. **A control is not done because code exists.** It is done when a test, a configuration, or a code path proves it works.

## When to Use

- Starting a new app, feature, or integration and deciding what security is required.
- The user asks whether the app is secure, asks for a security audit, or asks to find security bugs.
- Before a launch, a public release, or a move from prototype to production.
- Reviewing AI-generated or rapidly built code where security properties were never tested.
- A change touches auth, sessions, permissions, payments, uploads, secrets, external URLs, admin routes, or dependencies.
- The app gains a new data class, tenant, public endpoint, or third-party integration.

## When Not to Use

- **Formal threat model, architecture review, compliance, or incident response:** use `security`.
- **Code-level OWASP patterns** — password hashing, JWT, CSP, Zod, parameterized SQL: use `secure-code-guardian`.
- **Authentication flows** — OAuth/OIDC, sessions, MFA: use `authentication`.
- **A pure diff review:** use `code-reviewer`, and apply this skill as the security lens.
- **India DPDP compliance:** use `dpdpa-compliance`.
- **A throwaway local prototype with no real users, data, or secrets:** L0 only. Do not impose production controls on code that will be deleted.

## The Level Model

| Level | App profile | Mandatory floor | Typical target |
|---|---|---|---|
| **L0** | Local prototype, spike, throwaway | No real secrets, no real user data, never deployed as-is | Never a production target |
| **L1** | Any internet-facing production app | Core auth, server-side authz, input safety, HTTPS, secrets, safe errors, security tests | Mandatory for every production app |
| **L2** | Accounts or ordinary user data | L1 + advanced identity, central authz, monitoring, SAST/DAST | Normal target for login/user-data apps |
| **L3** | Sensitive data, payments, multi-tenant, privileged admin | L2 + WAF/DDoS, segmentation, KMS/rotation, containment, pen testing | High-assurance SaaS and commerce |
| **L4** | Business-critical, real customers, significant financial impact | L3 + secure SDLC, supply chain, incident response, operational security | Business-critical production |
| **L5** | Security, financial, or health infrastructure; high-value target | L4 + independent validation, SIEM, disaster recovery, formal assurance | Rare; justified by the threat model |

L1 is a floor. L2 is the normal production target once an app has accounts or stores user data. L3–L5 are risk decisions, not badges.

### How to Choose the Level

Ask in order. The highest triggered level wins.

1. **Is it not deployed, with no real users, data, or secrets?** → L0.
2. **Is it internet-facing and production-bound?** → at least L1.
3. **Does it have accounts, sessions, or ordinary user data?** → at least L2.
4. **Does it handle payments, health data, government IDs, credentials, multi-tenant data, or privileged admin actions?** → at least L3.
5. **Would a breach threaten the business, safety, or significant money?** → L4.
6. **Is it security infrastructure, a regulated high-assurance system, or a high-value target?** → L5.

Rules:

- If unsure between two levels, choose the higher one and write down why.
- Do not default to L5. Over-securing a landing page wastes effort and hides the controls that matter.
- Some controls do not apply — a static site has no login. Mark each as `N/A — <reason>`; do not silently skip it.
- Record the chosen level and the trigger in the task summary or the project's security document.

## Initial Build Protocol

Apply security before feature code, not after launch.

1. **Classify** the app and write: `Security level: Lx — <trigger>`.
2. **Apply the L1 floor** for every applicable area (table below).
3. **Add the level-specific controls** from [references/security-levels.md](references/security-levels.md) before the app ships.
4. **Add security tests** while the feature is fresh: cross-user access, missing permission, injection, invalid input, abuse cases.
5. **Record the baseline** in the project's security document or task summary.
6. **Re-evaluate** when auth, data class, tenancy, payments, uploads, public endpoints, or dependencies change.

### The L1 Floor

| Area | Mandatory controls |
|---|---|
| **Authentication & accounts** | Secure signup/login/logout; Argon2id or a strong bcrypt configuration; brute-force limits; session rotation; logout invalidation; short-lived single-use reset tokens; MFA for admin; no account enumeration |
| **Authorization** | Server-side check on every protected operation; ownership and tenant scoping; RBAC; default deny; admin actions separately protected; never rely on a hidden frontend button |
| **Input & injection** | Server-side schema validation; parameterized queries or safe ORM patterns; context-aware output encoding; command/path/SSRF protection; upload type/size/content/location checks; client validation is not a boundary |
| **API & network** | HTTPS/TLS; CORS allowlist; rate limits on auth and expensive endpoints; body/upload/time limits; webhook signature verification; CSP, HSTS, `X-Content-Type-Options`, frame protection; no privileged keys in the client bundle |
| **Data & secrets** | Database not publicly reachable; least-privilege DB account; encryption in transit and at rest where appropriate; secrets in a manager or env, never in code, Git history, logs, or client env; encrypted backups; return only the data the caller needs |
| **Errors & config** | No stack traces, SQL, secrets, or internal paths in production errors; debug off; `HttpOnly`/`Secure`/`SameSite` cookies; CSRF protection for cookie-auth state changes; no default credentials, test accounts, or debug endpoints; dev/test configuration disabled |
| **Security testing** | Auth and authz tests; cross-user, injection, and invalid-input tests; dependency and secret scanning; CI checks that block critical failures; security event logging without passwords, tokens, or sensitive payloads |

**Evidence rule:** for each control, name the evidence — a test name, a config line, a middleware, or a code path. "Implemented" without evidence is a claim, not a control.

### Baseline Record

Use this when the project has a security document; otherwise include it in the task summary.

```markdown
## Security Baseline
- Level: L2
- Trigger: user accounts + profile data
- Verified controls: authz on all /api routes; Argon2id; rate limit on /login; CSP; dependency scan
- Open gaps: password reset rate limit (owner: ..., target: ...)
- Accepted risks: analytics cookie lacks SameSite=None rationale (accepted by: ...)
- Next review: when payments or tenant data are added
```

## Security Bug Hunt

When the user asks to find security bugs, do not run a single scanner and call it an audit. Reason about the code, the data flow, and the business rules. Use `security` for deep analysis and `secure-code-guardian` for implementation patterns.

### 1. Set Scope and Permission

- Audit only systems you own or have written permission to test.
- Do not run destructive payloads, denial-of-service tests, or data exfiltration against production.
- Prefer local or staging. Prove a finding with the minimum safe evidence.
- If you find a live secret, do not print it. Record its location and type, and recommend immediate rotation.

### 2. Inventory the Attack Surface

Map: routes and server actions, auth guards, roles and tenants, data models, raw queries, HTML rendering, uploads/downloads, outbound fetches and webhooks, admin/debug surfaces, secrets and configuration, dependencies, logging, and deployment settings.

### 3. Audit by Domain

Use [references/security-audit-playbook.md](references/security-audit-playbook.md) for checks, safe proofs, and common AI-code failures across:

1. Authentication and sessions.
2. Authorization, IDOR/BOLA, and tenant isolation.
3. Injection — SQL, NoSQL, command, template, LDAP.
4. XSS, CSRF, and CORS.
5. SSRF, path traversal, and file uploads.
6. Secrets and configuration.
7. Cryptography and data protection.
8. Abuse, rate limits, and business logic.
9. Dependencies and supply chain.
10. Logging, errors, and monitoring.

### 4. Report Findings, Not Impressions

Use this format for every finding:

```markdown
### [HIGH] Cross-user invoice access via invoiceId
- ID: SEC-001
- Confidence: Confirmed
- Location: app/api/invoices/[id]/route.ts:24
- Evidence: query filters by invoiceId only; no tenant or user scope
- Exploit path: authenticated user changes the invoiceId in the request
- Impact: reads another customer's invoice and billing data
- Fix: scope the query by tenantId and userId; return 404 on no match
- Verification: two-user test; user B receives 404 for user A's invoice
- Class: OWASP A01 Broken Access Control / CWE-639
```

Rules:

- **Severity:** Critical, High, Medium, or Low. Add confidence: Confirmed, Likely, or Needs validation.
- **Evidence:** file and line, route, or code path. Do not paste secrets or personal data.
- **Exploit path:** who can do what, under which preconditions. A theoretical issue without a plausible path is hardening, not a vulnerability.
- **Fix:** specific and minimal. "Add authorization" is not a fix; "scope the query by tenantId" is.
- **Verification:** the test that proves the fix.
- **Coverage:** state what you reviewed and what you did not. Never claim the app is 100% secure.

### 5. Prioritize and Propose

Order by exploitability × impact:

| Priority | Meaning | Action |
|---|---|---|
| **Fix now** | Critical or High, exploitable in the current build | Block release until fixed |
| **Before release** | High or Medium with a realistic path | Fix in the current cycle |
| **Backlog** | Low or hardening with no current exploit | Schedule with an owner |
| **Accept** | Risk is understood and accepted | Record rationale, owner, and review date |

End with a remediation plan, not just a list. Include the fix order, effort, and the regression tests to add.

## Proactive Security Reminders

Security reminders are **event-driven**, not a timer and not a recurring lecture.

### When to Raise One

Raise a reminder when a change touches:

- Authentication, sessions, password reset, or MFA.
- Roles, permissions, ownership, tenant scoping, or admin routes.
- Payments, refunds, credits, or financial calculations.
- File uploads, downloads, parsers, or document conversion.
- User-controlled URLs, webhooks, or outbound requests.
- Raw SQL/NoSQL, shell commands, templates, or HTML rendering.
- Secrets, environment variables, keys, or configuration.
- A new dependency, SDK, integration, or public endpoint.
- CORS, CSP, cookies, or security headers.
- Logging or error handling that may include PII or credentials.

### How to Raise It

Keep it to four lines, next to the relevant work:

```text
SECURITY CHECK — invoice download
Risk: any authenticated user can read another tenant's invoice by changing the ID.
Do: scope the query by tenantId and return 404 when it does not match.
Verify: add a two-user authorization test.
```

### Reminder Budget

- **Once per issue per session.** Do not repeat the same warning.
- **Do not block** the user's task. State the risk and the fix; let them decide.
- **Escalate** to a finding with severity if the current code is exploitable.
- **Record accepted risk** if the user declines: reason, owner, date, and review trigger. Do not silently drop it.
- **No generic warnings.** "Be careful with security" is noise. Tie every reminder to a specific threat and a concrete action.
- **End-of-task delta:** close with one line:
  `Security: L2 | 3 controls verified | 1 open gap: reset rate limit | Re-check after auth changes`

## Top 10 Vibe-Coding Failures

| # | Failure | What it looks like | First check |
|---|---|---|---|
| 1 | Frontend-only authorization | Admin button hidden; API still allows the action | Every server handler checks permission |
| 2 | Hardcoded secrets | Keys in source, Git history, or the client bundle | Secret scan + built client bundle |
| 3 | Broken tenant isolation | Changing an ID reads another tenant's record | Every query scoped by tenant |
| 4 | Blind trust in AI code | It compiles and looks right; security was never tested | Authz and abuse-case tests |
| 5 | Unsafe dependencies | Outdated, vulnerable, or typosquatted packages | Lockfile + audit + package provenance |
| 6 | Insecure production config | Debug on, public DB, wildcard CORS, default credentials | Deployment config review |
| 7 | Unsafe uploads | No type/size/content check; files stored executable/public | Upload validation and isolated storage |
| 8 | No regression tests | A fix reopens an old vulnerability | Test per fixed finding |
| 9 | No abuse-case tests | Only happy paths are tested | Unauthorized, malformed, race, replay |
| 10 | No detection or recovery | Breach is invisible; keys cannot be revoked; restore untested | Logs, alerts, rotation, restore drill |

## Routing

| Need | Use |
|---|---|
| Deep threat model, security architecture, compliance, IR, supply chain | `security` |
| Password hashing, JWT, CSP, Zod, parameterized SQL, headers | `secure-code-guardian` |
| Login, OAuth/OIDC, sessions, MFA, password reset | `authentication` |
| Backend production rigor and error envelopes | `backend-craft` |
| Diff review and findings report | `code-reviewer` |
| India DPDP compliance | `dpdpa-compliance` |
| Infrastructure, cloud, monitoring, reliability | `devops`, `cloud`, `monitoring`, `sre-engineer` |
| Database design and access | `database-architecture`, `database-performance` |
| Test strategy and coverage | `testing`, `test-master` |

## Anti-Patterns

- **Security theater.** A checklist with no evidence, a scanner with no reasoning, or a policy nobody enforces.
- **Over-leveling.** L5 controls on a landing page; complexity without a threat.
- **Under-leveling.** L1 only for an app that stores payments or tenant data.
- **Frontend-only authorization.** Hiding a button is not access control.
- **Secrets in the wrong place.** Source code, Git history, logs, error messages, or the client bundle.
- **Scanner-only audits.** SAST and dependency scans miss IDOR, business logic, and tenant isolation.
- **Fear-driven warnings.** Vague doom erodes trust; specific risks get fixed.
- **Reminder spam.** Repeating the same warning in every response.
- **Blocking without a path.** A finding without a fix is only half the job.
- **Claiming "100% secure".** State coverage and limits instead.

## Rationalizations to Reject

| Excuse | Reality |
|---|---|
| "We'll add security later." | Later competes with the next feature. Apply the L1 floor before launch. |
| "We're too small to be targeted." | Scanners and credential stuffing do not check company size. |
| "The AI wrote it, so it is probably fine." | Generated code compiles; security properties still need tests. |
| "It is behind login." | Authentication is not authorization. Check IDOR and tenant scope. |
| "The button is hidden." | The API is the boundary. Hide nothing that the server does not enforce. |
| "The scanner passed." | Scanners miss business logic, access control, and chained issues. |
| "Let's do L5 everywhere." | Proportional security. Spend effort where the blast radius is. |
| "No findings means secure." | No findings means the audit had limits. Say what they were. |
| "Just this once." | Record it as accepted risk with an owner and a review date. |

## Verification Checklist

- [ ] The app level is recorded with the trigger that set it.
- [ ] Every applicable L1 control is verified by a test, config, or code path.
- [ ] Level-specific controls are implemented, or gaps are recorded with an owner and target.
- [ ] Every protected operation enforces authorization server-side.
- [ ] A cross-user / cross-tenant test exists for object access.
- [ ] Input is validated at the boundary; queries are parameterized.
- [ ] Secrets are absent from code, Git history, logs, errors, and the client bundle.
- [ ] Cookies, CORS, CSP, and security headers are configured.
- [ ] Production errors do not leak internals.
- [ ] Dependency and secret scans run in CI.
- [ ] Security tests cover unauthorized, malformed, and abuse cases.
- [ ] Findings include severity, evidence, exploit path, fix, and verification.
- [ ] Reminders fire only on relevant changes and are not repeated.
- [ ] Accepted risks are recorded with rationale and review date.
- [ ] No destructive test or real-data extraction was run against production.

## Related Skills

- `security` — deep architecture, threat modeling, infrastructure, supply chain, IR, compliance.
- `secure-code-guardian` — implementation patterns for auth, input, headers, and OWASP.
- `authentication` — auth flows, sessions, OAuth/OIDC, MFA.
- `backend-craft` — production backend validation and error handling.
- `code-reviewer` — structured diff review.
- `dpdpa-compliance` — India's DPDP Act obligations.
- `devops`, `cloud`, `monitoring`, `sre-engineer` — infrastructure and operations.
- `testing`, `test-master` — security test strategy and coverage.
