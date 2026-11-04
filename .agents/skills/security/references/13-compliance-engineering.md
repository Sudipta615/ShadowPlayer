# Compliance Engineering

This reference covers **Phase 14: Compliance Engineering**. See the main SKILL.md for the phase summary and link.

---

32. **Translate compliance requirements into engineering controls.** Compliance frameworks are not security checklists — they are sets of principles that must be implemented through concrete engineering controls. For each applicable framework:

    **SOC 2** (most common for SaaS):
    - Trust Service Criteria: Security (mandatory), Availability, Processing Integrity, Confidentiality, Privacy (select relevant criteria).
    - Key engineering controls: Access management (RBAC, MFA, access reviews), change management (CI/CD with approvals, audit trail), encryption (in transit and at rest), monitoring (log aggregation, alerting, incident response), vendor management (third-party risk assessment), vulnerability management (scanning, remediation SLAs).
    - Evidence collection: Automate evidence collection. Use infrastructure-as-code (demonstrates change management), CI/CD logs (demonstrates deployment process), access management exports (demonstrates access reviews), and monitoring dashboards (demonstrates continuous monitoring).

    **GDPR**:
    - Key engineering controls: Data minimization (collect only what is necessary), purpose limitation (use data only for stated purposes), data subject rights implementation (export, deletion, correction APIs), consent management, data processing records, privacy impact assessments, data breach notification process (72-hour requirement), cross-border data transfer controls (Standard Contractual Clauses, adequacy decisions), data anonymization for analytics.
    - **Right to erasure implementation**: Design the system to delete all personal data for a specific user across all services, databases, backups, logs, and third-party systems. This is technically complex — map all data stores containing PII and design a coordinated deletion process. For backups, accept that data in backups will expire naturally with the backup retention period (this is generally acceptable under GDPR, but document the approach).

    **HIPAA** (healthcare data):
    - Key engineering controls: Encryption at rest and in transit (required), access controls with audit logging, unique user identification, automatic logoff (session timeout), audit controls (comprehensive logging of PHI access), integrity controls, transmission security, BAAs (Business Associate Agreements) with all vendors handling PHI.
    - PHI must be identified, tracked, and protected across all systems. Create a PHI data flow map.

    **PCI-DSS** (payment card data):
    - **Reduce scope aggressively**: Use a third-party payment processor (Stripe, Adyen, Braintree) with tokenization so that your systems never handle raw card numbers. This dramatically reduces PCI scope.
    - If card data must be handled: network segmentation isolating the cardholder data environment (CDE), encryption, key management, access controls, logging, vulnerability management, penetration testing — all scoped to the CDE.

    **General approach**:
    - Map each compliance requirement to a specific engineering control.
    - Automate evidence collection for each control (manual evidence gathering is error-prone and expensive).
    - Monitor compliance continuously (compliance-as-code) rather than annually. Tools: Vanta, Drata, Secureframe, or custom automation.
    - Treat compliance requirements as minimum security baseline — good security practice often exceeds compliance requirements.