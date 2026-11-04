# Security Testing

This reference covers **Phase 11: Security Testing**. See the main SKILL.md for the phase summary and link.

---

27. **Design the security testing strategy.** Security testing must be layered, automated, and continuous — not a one-time annual event:

    **Automated testing (run continuously in CI/CD)**:
    - **SAST**: Analyze source code for vulnerability patterns without executing it. Configure custom rules for application-specific security patterns. Address findings in the same sprint — do not accumulate a backlog of SAST findings. Tune rules to minimize false positives (false positives erode developer trust and lead to ignored findings).
    - **DAST (Dynamic Application Security Testing)**: Test the running application by sending crafted requests to discover vulnerabilities (injection, XSS, auth bypass, information disclosure). Run against a staging environment after deployment. Tools: OWASP ZAP (open source, recommended for automation), Burp Suite (commercial, recommended for manual testing), Nuclei (for custom vulnerability templates).
    - **IAST (Interactive Application Security Testing)**: Instrument the running application to detect vulnerabilities during normal test execution (including functional and integration tests). Provides better accuracy than SAST (fewer false positives) and better coverage than DAST (sees internal execution paths). Tools: Contrast Security, Datadog IAST.
    - **API security testing**: Specifically test APIs for: broken authentication, broken authorization (BOLA/IDOR — attempt to access other users' resources), injection, rate limiting bypass, and business logic flaws. Tools: OWASP ZAP API scan, Postman security tests, custom scripts.
    - **Infrastructure scanning**: Continuously scan cloud configuration for security misconfigurations. Tools: AWS Security Hub, GCP Security Command Center, Prowler, ScoutSuite.

    **Manual testing (periodic)**:
    - **Penetration testing**: Engage external penetration testers at least annually, and after any major architectural change. Define the scope (which systems, which attack scenarios), rules of engagement, and reporting format. Address Critical and High findings before the pentest report is finalized. Track remediation of all findings.
    - **Security code review**: For security-critical components (authentication, authorization, cryptography, input handling, payment processing), perform dedicated security-focused code reviews by engineers with security expertise. This is separate from standard code review — it specifically looks for vulnerability patterns, logic flaws, and race conditions.
    - **Threat model review**: Review and update the threat model (see the Context and Threat Modeling reference) when the system architecture changes, new features are added, or new threat intelligence is available.

    **Bug bounty program** (for mature security organizations):
    - Consider a bug bounty program (HackerOne, Bugcrowd, or self-hosted) for internet-facing applications. Continuous testing by external researchers complements internal testing.
    - Define clear scope, rules of engagement, severity-based reward tiers, and a response SLA.
    - Prerequisite: The organization must have the capacity to triage, validate, and remediate reported vulnerabilities promptly. A bug bounty without remediation capacity creates frustration and reputational risk.

28. **Design security unit and integration tests.** Security-specific tests must be part of the standard test suite:

    **Authorization tests**:
    - For every endpoint, write tests that verify: unauthenticated requests are rejected (401), unauthorized requests are rejected (403), and users can only access resources they own (IDOR prevention).
    - Test both positive cases (user CAN access their own resource) and negative cases (user CANNOT access another user's resource). The negative tests are more important.
    - For multi-tenant systems: test that Tenant A's credentials cannot access Tenant B's data.

    **Input validation tests**:
    - For every endpoint that accepts input, test with: maximum-length strings, special characters (`'`, `"`, `<`, `>`, `--`, `\0`), Unicode edge cases, type mismatches (string where integer expected), negative numbers, extremely large numbers, empty strings vs. null vs. missing fields.
    - These are not functional tests — they are specifically testing that the application handles malicious or malformed input safely.

    **Cryptography tests**:
    - Verify that passwords are hashed with the correct algorithm and parameters.
    - Verify that sensitive fields are encrypted before storage and decrypted correctly on retrieval.
    - Verify that tokens with tampered signatures are rejected.
    - Verify that expired tokens are rejected.

    **Rate limiting tests**:
    - Verify that rate limits are enforced (send N+1 requests and verify the last one is rejected with 429).
    - Verify rate limit headers are included in responses.