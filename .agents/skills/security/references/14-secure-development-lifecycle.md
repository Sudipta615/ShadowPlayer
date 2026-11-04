# Secure Development Lifecycle (SDL)

This reference covers **Phase 15: Secure Development Lifecycle (SDL)**. See the main SKILL.md for the phase summary and link.

---

33. **Design security integration into the development workflow.** Security must be integrated into the engineering process, not bolted on after development:

    **Security requirements in design phase**:
    - Every feature design document must include a security section: data classification, authentication/authorization requirements, input validation approach, and threat considerations. If the feature handles sensitive data or modifies the trust boundary, require a lightweight threat model.
    - Define security champions on each engineering team — engineers with security interest/training who review designs and code for security issues. Security champions bridge the gap between the security team and engineering teams.

    **Secure coding guidelines**:
    - Maintain language-specific secure coding guidelines for each technology in use. Cover: input validation, output encoding, authentication/authorization patterns, error handling, logging (what to log and what not to log), cryptography (approved algorithms and libraries), dependency management.
    - Make these guidelines actionable, with code examples (both secure and insecure patterns), not abstract principles.
    - Enforce coding guidelines through SAST rules customized to your codebase.

    **Code review for security**:
    - Standard code review should include security awareness: reviewers should look for common vulnerability patterns.
    - For security-critical changes (authentication, authorization, crypto, input handling, financial logic), require review by a security champion or security team member.
    - Use a security review checklist for high-risk changes:
      - [ ] Input validation on all external inputs.
      - [ ] Parameterized queries (no string concatenation in SQL).
      - [ ] Authorization checks on every resource access.
      - [ ] No sensitive data in logs or error responses.
      - [ ] No hardcoded secrets.
      - [ ] Dependencies reviewed for known vulnerabilities.
      - [ ] Error handling does not leak internal details.
      - [ ] Rate limiting on sensitive endpoints.

    **Security training**:
    - Annual security training for all engineers: OWASP Top 10, common vulnerability patterns, secure coding for the team's specific technology stack.
    - Hands-on training (CTF exercises, vulnerable application labs like OWASP Juice Shop, WebGoat) is more effective than slide-based training.
    - Post-incident training: After a security incident or near-miss, share the (anonymized if necessary) technical details with the engineering team as a learning opportunity.

34. **Design security feature flags and kill switches.** For security-sensitive features:
    - Deploy behind feature flags that can be disabled instantly without a full deployment. If a new authentication flow has a vulnerability, disable it in seconds via the feature flag.
    - Implement kill switches for: third-party integrations (disable if the third party is compromised), file upload (disable if an upload-based attack is in progress), specific API endpoints (disable a vulnerable endpoint while patching), user registration (disable during a mass spam registration attack).
    - Kill switches must be operable by on-call engineers without requiring a code deployment or senior approval during active incidents.