# Security Levels L0–L5

Use this reference after the app's level has been selected in `SKILL.md`. Apply every control that matches the app's exposure, mark the rest `N/A — <reason>`, and record the evidence for each implemented control.

**Evidence standard:** a control is verified by one of:

- a test that exercises the attack path,
- a configuration line that can be inspected,
- a middleware, guard, or code path that enforces it,
- a platform guarantee that is documented and actually enabled.

A checked box without evidence is not a control.

## Level Selector

| App type | Practical target |
|---|---|
| Local prototype, spike, or throwaway | L0 |
| Public static or marketing site with no accounts | L1 applicable controls |
| Login and ordinary user data | L1 + L2 |
| SaaS or multi-tenant application | L2 + selected L3 |
| E-commerce or financial workflows | L2–L4, by data and blast radius |
| Security software or sensitive infrastructure | L3–L5, by threat model |
| Health, financial, or other regulated high-assurance system | L4–L5, with formal assurance |

If a single criterion pulls the app up a level, the app is at the higher level. A marketing site with an admin CMS is not a static site. A B2B tool with one tenant is still an L2 app once it has accounts.

## Level 0 — Local Prototype

Use only for code that will be deleted, never deployed, and never given real credentials or real user data.

- [ ] No production secrets, API keys, or customer data in the repository.
- [ ] No real accounts or personal data in fixtures; use synthetic data.
- [ ] Runs on localhost or a private development environment only.
- [ ] No shared staging/production credentials in the local environment.
- [ ] Before the prototype gets a real user, a real deployment, or a real secret, promote it to L1.

L0 is a scope decision, not a security posture. It ends the moment the app becomes real.

## Level 1 — Core Essential Security

Mandatory for any internet-facing production app. Apply only the controls that fit the app; a static site has no login but still needs transport, headers, secrets, and configuration hygiene.

### 1. Authentication and Account Security

- [ ] Secure signup, login, and logout flows; no custom crypto.
- [ ] Passwords hashed with Argon2id or a strong bcrypt cost; never plaintext or reversible encryption. *Evidence: hash call + cost/parameters.*
- [ ] Brute-force protection on login, reset, and OTP endpoints. *Evidence: rate-limit middleware + test.*
- [ ] Session identifier rotated after login; session invalidated on logout. *Evidence: session store behavior + test.*
- [ ] Password reset tokens are random, short-lived, single-use, and bound to the account.
- [ ] MFA available, and enforced for administrators and sensitive accounts.
- [ ] Login and reset responses do not reveal whether an account exists.
- [ ] Credential recovery cannot be used to take over an account through predictable questions, weak email flows, or reusable links.

**Common AI failure:** a working login form with no rate limit, no session rotation, and a reset token that never expires.

### 2. Authorization and Access Control

- [ ] Every protected API, server action, and server-rendered operation checks permission. *Evidence: guard/middleware coverage + test.*
- [ ] Object access is scoped to the owner or tenant; changing an ID cannot read another user's record (IDOR/BOLA).
- [ ] Roles are enforced server-side; RBAC or an equivalent policy layer exists.
- [ ] Admin and privileged actions have separate authorization, not a reused user check.
- [ ] Default deny: no permission means no access.
- [ ] Mass assignment is blocked; users cannot set `role`, `isAdmin`, `tenantId`, or protected fields through a request body.
- [ ] Frontend hiding is treated as UX only; the server is the boundary.

**Common AI failure:** `findById(req.params.id)` with no `userId` or `tenantId` filter.

### 3. Input Validation and Injection Protection

- [ ] Every incoming request is validated server-side against a schema; unknown fields are rejected or stripped.
- [ ] SQL uses parameterized queries or safe ORM query builders; no string concatenation.
- [ ] NoSQL queries reject operator injection (`$ne`, `$gt`, `$where`) from user input.
- [ ] OS commands never receive concatenated user input; prefer APIs over shells.
- [ ] Template injection is prevented by using logic-less templates or escaping untrusted values.
- [ ] Path traversal is blocked by resolving paths inside an allowed root.
- [ ] SSRF is blocked by validating destinations, blocking private/link-local/metadata ranges, and restricting egress.
- [ ] File uploads validate type, size, content, filename, and storage location; files are not served from an executable path.
- [ ] Client-side validation is not treated as a security boundary.

**Common AI failure:** a Zod schema on the client, a raw query on the server, and an upload endpoint that trusts `Content-Type`.

### 4. API and Network Security

- [ ] HTTPS/TLS is enforced; HTTP redirects or is refused.
- [ ] CORS uses an explicit allowlist; wildcard origins are not combined with credentials.
- [ ] Rate limits cover login, signup, reset, OTP, search, export, and expensive endpoints.
- [ ] Request body, upload, and execution-time limits are set.
- [ ] Webhooks and machine-to-machine calls authenticate and verify signatures where applicable.
- [ ] Security headers are present: CSP, HSTS, `X-Content-Type-Options`, frame protection, and a referrer policy.
- [ ] Privileged API keys and service credentials are not shipped in the browser bundle.
- [ ] Cookies use `HttpOnly`, `Secure`, and an appropriate `SameSite` value.

### 5. Database and Sensitive Data Protection

- [ ] The database is not directly reachable from the public internet unless explicitly required and tightly restricted.
- [ ] The application uses a least-privilege database account; migrations use a separate account where possible.
- [ ] Data is encrypted in transit; sensitive stored data has appropriate encryption at rest.
- [ ] Passwords, API secrets, private keys, and session tokens are stored securely.
- [ ] Secrets are absent from source, Git history, logs, error responses, and client-exposed environment variables.
- [ ] Backups are access-controlled, encrypted, and restorable.
- [ ] Responses return only the fields the caller needs; no over-fetching of PII or internal fields.

### 6. Error Handling and Secure Configuration

- [ ] Production errors do not expose stack traces, SQL, secrets, internal paths, or framework versions.
- [ ] Debug mode, verbose errors, and development tooling are disabled in production.
- [ ] CSRF protection exists for cookie-authenticated state-changing requests.
- [ ] Default credentials, test accounts, seed admin users, and debug endpoints are removed.
- [ ] Development and test configuration cannot be selected in production.
- [ ] Dependency and configuration vulnerabilities have at least a basic automated check.

### 7. Essential Security Testing

- [ ] Unit or integration tests cover authentication and authorization.
- [ ] Tests cover cross-user access, missing permission, injection, and invalid input.
- [ ] Dependency vulnerability scanning runs in CI.
- [ ] Secret scanning runs before merge or in CI.
- [ ] CI blocks release on critical security failures.
- [ ] Security events are logged without passwords, tokens, or sensitive payloads.

## Level 2 — Strong Application Security

The normal target for an app with accounts or user data. L2 assumes L1 is complete.

### 1. Advanced Identity Security

- [ ] MFA enforced for administrators and sensitive roles.
- [ ] Suspicious login detection (new device, impossible travel, repeated failures).
- [ ] Users can revoke sessions after a password change or suspected compromise.
- [ ] Sensitive actions require re-authentication.
- [ ] Session expiry, idle timeout, and concurrent-session policy exist.
- [ ] OAuth/OIDC uses `state`, `nonce`, and PKCE where applicable.
- [ ] Account recovery is threat-modeled and cannot bypass MFA or email ownership.

### 2. Advanced Access Control

- [ ] Authorization is centralized in middleware or a policy layer, not copied per handler.
- [ ] Resource-level and field-level authorization is enforced.
- [ ] Multi-tenant isolation has explicit cross-tenant tests.
- [ ] Privilege-escalation paths are tested, including role and tenant changes.
- [ ] Admin actions use least privilege and approval where appropriate.
- [ ] Mass assignment and protected-field changes are tested, not only reviewed.

### 3. Application Hardening

- [ ] A strict Content Security Policy is deployed and tested; no unsafe-inline unless justified.
- [ ] Outbound network access is restricted and destinations are validated for SSRF.
- [ ] Untrusted uploads are processed in isolation and stored outside executable/public paths.
- [ ] XML parsing disables external entities (XXE) when XML is accepted.
- [ ] Deserialization does not load untrusted objects or classes.
- [ ] Cache poisoning and sensitive-response caching are reviewed.
- [ ] Race conditions, duplicate requests, replay, and business-logic abuse are tested.

### 4. Security Monitoring

- [ ] Failed logins and repeated authorization failures are logged.
- [ ] Admin privilege changes and sensitive data exports are recorded.
- [ ] Logs are centralized and access-controlled.
- [ ] Alerts exist for suspicious activity, not only uptime.
- [ ] Credentials, tokens, and unnecessary personal data are redacted from logs.
- [ ] Timestamps, request IDs, actor IDs, and relevant context support investigation.

### 5. Automated Security Testing

- [ ] SAST runs on the codebase.
- [ ] DAST runs against a running environment.
- [ ] Dependency/SCA scanning is part of CI.
- [ ] Container and infrastructure configuration scanning is used where applicable.
- [ ] API security tests cover authentication, authorization, and malformed requests.
- [ ] Negative tests cover unauthorized, malformed, and malicious input.
- [ ] Threat modeling is performed for critical flows: auth, payments, uploads, admin.

## Level 3 — Advanced Security and Attack Resistance

For sensitive data, payments, multi-tenancy, or privileged administration. L3 assumes L2 is complete.

### 1. Advanced Protection

- [ ] WAF or equivalent request filtering is deployed where it adds value.
- [ ] DDoS protection and traffic filtering are in place.
- [ ] Bot detection covers abusive automation on auth, signup, and scraping.
- [ ] Rate limiting is risk-based, not only a fixed request count.
- [ ] Network segmentation and private service networking isolate data stores.
- [ ] Sensitive service-to-service calls use mTLS or equivalent authentication.
- [ ] Security-sensitive operations have separate permissions and approval workflows.

### 2. Data Protection and Cryptography

- [ ] Keys live in a KMS or secrets vault.
- [ ] Encryption keys are separated from encrypted data.
- [ ] Key rotation and revocation procedures exist and are tested.
- [ ] Sensitive data has a retention and secure-deletion policy.
- [ ] Backups are protected independently from application credentials.
- [ ] Backup restore and disaster-recovery procedures are tested.
- [ ] Algorithms, modes, and key sizes receive expert review.

### 3. Exploit Containment

- [ ] The app runs with minimum OS/container privileges.
- [ ] Filesystems are read-only or restricted to known writable paths where practical.
- [ ] Untrusted file parsing and document conversion run in isolated workers.
- [ ] Egress filtering prevents a compromised component from reaching arbitrary destinations.
- [ ] Production, staging, and development are isolated.
- [ ] Databases, object storage, and internal services enforce independent authorization.
- [ ] CPU, memory, storage, and concurrency limits resist resource exhaustion.

### 4. Security Verification

- [ ] Threat models cover auth, payments, uploads, and admin workflows.
- [ ] Manual penetration testing is performed.
- [ ] Business-logic abuse is tested, not only technical vulnerabilities.
- [ ] Cross-tenant isolation is tested.
- [ ] Fuzzing covers parsers, APIs, and complex input handlers.
- [ ] Security regression tests cover previously fixed vulnerabilities.
- [ ] High-risk findings are resolved before release.

## Level 4 — Production-Grade Security Engineering

For business-critical apps with real customers and significant financial or operational impact. L4 assumes L3 is complete.

### 1. Secure Software Development Lifecycle

- [ ] Written security requirements exist.
- [ ] The threat model is maintained alongside the architecture.
- [ ] Sensitive changes require security review.
- [ ] Branches are protected; pull-request review is mandatory.
- [ ] CI/CD has security gates.
- [ ] Deployment permissions are separate from development permissions.
- [ ] Production changes have an audit trail.
- [ ] High-risk releases require security sign-off.

### 2. Software Supply-Chain Security

- [ ] Dependencies are scanned for known vulnerabilities and malicious packages.
- [ ] Lockfiles are maintained and reproducible builds are preferred.
- [ ] Transitive dependencies and dependency updates are reviewed.
- [ ] Build and CI credentials follow least privilege.
- [ ] Secret scanning covers history, and leaked credentials have a revocation process.
- [ ] An SBOM is generated where appropriate.
- [ ] Release artifacts are signed and verified.
- [ ] Build provenance and artifact integrity are checked.
- [ ] Third-party integrations and SDK permissions are reviewed.

### 3. Monitoring and Incident Response

- [ ] Security monitoring is centralized.
- [ ] Alerts have severity and actionable context.
- [ ] An incident response playbook exists.
- [ ] Credential compromise and key leakage have emergency rotation procedures.
- [ ] Account and session revocation is possible.
- [ ] Data-breach investigation procedures exist.
- [ ] Backup restoration and recovery objectives are tested.
- [ ] Post-incident root-cause analysis produces regression tests.

### 4. Operational Security

- [ ] Infrastructure as Code is security-scanned where applicable.
- [ ] Cloud IAM follows least privilege.
- [ ] Development, staging, and production use separate credentials.
- [ ] Access reviews happen regularly.
- [ ] Patch management and vulnerability remediation SLAs exist.
- [ ] Public cloud storage, databases, and admin panels are audited for exposure.
- [ ] Security configuration drift is detected.

## Level 5 — Maximum Assurance and Continuous Security

For security products, financial or health infrastructure, and other high-value targets. L5 is a business decision justified by the threat model, not a default.

### 1. Independent Security Validation

- [ ] Independent third-party penetration testing is performed.
- [ ] Red-team exercises are used where justified.
- [ ] Experienced security engineers perform threat modeling.
- [ ] Critical attack paths are manually verified.
- [ ] Security architecture is reviewed before major releases.
- [ ] High-risk vulnerabilities receive independent retesting.
- [ ] Formal audits cover relevant compliance requirements.

### 2. Advanced Attack Detection

- [ ] SIEM or equivalent centralized security analytics is in place.
- [ ] Behavioral anomaly detection covers accounts and privileged actions.
- [ ] Suspicious privilege changes trigger automated alerts.
- [ ] Unusual data exports and mass-access patterns are detected.
- [ ] Security logs are tamper-resistant where appropriate.
- [ ] Incident response exercises and forensic readiness are maintained.

### 3. Advanced Resilience

- [ ] The disaster recovery plan is tested.
- [ ] Backups are independent and protected.
- [ ] RTO and RPO are defined.
- [ ] Disaster recovery drills are run.
- [ ] Critical services have redundancy and failover.
- [ ] Compromised credentials and systems can be isolated quickly.
- [ ] Security failures have a safe fallback behavior.

### 4. Formal Assurance

- [ ] OWASP ASVS Level 3 requirements are met where applicable.
- [ ] Regulatory and industry-specific controls are implemented.
- [ ] Cryptographic architecture receives expert review.
- [ ] The secure development lifecycle produces evidence.
- [ ] A vulnerability disclosure policy exists.
- [ ] Security testing is responsible and coordinated.

## Mapping to Deep References

When a control needs design-level detail, use the existing `security` skill rather than expanding this reference:

| Area | Deep reference |
|---|---|
| Context and threat modeling | `security` → `references/01-context-and-threat-modeling.md` |
| Application security | `security` → `references/02-application-security.md` |
| Authorization | `security` → `references/03-authorization-access-control.md` |
| Data protection and cryptography | `security` → `references/04-data-protection-cryptography.md` |
| Secrets management | `security` → `references/05-secrets-management.md` |
| Infrastructure | `security` → `references/06-infrastructure-security.md` |
| Containers and runtime | `security` → `references/07-container-runtime-security.md` |
| CI/CD | `security` → `references/08-cicd-pipeline-security.md` |
| Supply chain | `security` → `references/09-supply-chain-security.md` |
| Security testing | `security` → `references/10-security-testing.md` |
| Vulnerability management | `security` → `references/11-vulnerability-management.md` |
| Monitoring and incident response | `security` → `references/12-monitoring-incident-response.md` |
| Compliance | `security` → `references/13-compliance-engineering.md` |
| Secure development lifecycle | `security` → `references/14-secure-development-lifecycle.md` |

Implementation patterns — password hashing, JWT validation, CSP, Zod, parameterized queries — belong to `secure-code-guardian`. Authentication flows belong to `authentication`.

## Official References

- OWASP Top 10: https://owasp.org/Top10/
- OWASP ASVS: https://owasp.org/www-project-application-security-verification-standard/
- OWASP Cheat Sheet Series: https://cheatsheetseries.owasp.org/
- CWE Top 25: https://cwe.mitre.org/top25/
- NIST Secure Software Development Framework: https://csrc.nist.gov/Projects/ssdf
- SLSA supply-chain framework: https://slsa.dev/

Use the current edition of each framework. A level is a planning aid, not a certification.
