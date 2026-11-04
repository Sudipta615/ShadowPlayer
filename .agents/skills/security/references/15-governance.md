# Security Governance and Continuous Improvement

This reference covers **Phase 16: Security Governance and Continuous Improvement**. See the main SKILL.md for the phase summary and link.

---

35. **Establish security metrics and reporting.** What gets measured gets managed. Track and report:

    **Vulnerability metrics**:
    - Open vulnerability count by severity.
    - Mean time to remediate (MTTR) by severity — trending over time.
    - Vulnerability SLA compliance percentage.
    - Dependency vulnerability count and age.
    - Percentage of services with up-to-date dependency scans.

    **Posture metrics**:
    - Percentage of services with all security controls implemented (encryption, logging, scanning, access control).
    - Cloud security posture score (from Security Hub, Cloud SCC, or equivalent).
    - MFA adoption rate (what percentage of users/admins have MFA enabled).
    - Percentage of secrets managed through the secrets manager (vs. hardcoded/unmanaged).
    - Percentage of container images passing security scans.

    **Process metrics**:
    - Number of security reviews completed per quarter.
    - Security training completion rate.
    - Incident response drill completion and results.
    - Time to detect (TTD) security events.
    - Time to respond (TTR) to security alerts.

    **Report regularly**: Monthly security metrics report to engineering leadership. Quarterly security posture review with executive stakeholders. Use metrics to prioritize security investments and demonstrate progress.

36. **Design the security review cadence.**
    - **Continuous**: Automated scanning (SAST, SCA, container scanning, cloud configuration scanning) runs on every build and daily.
    - **Per-change**: Security-relevant code changes reviewed by security champion. New dependencies reviewed for risk.
    - **Monthly**: Review open vulnerabilities, review security metrics, review access logs for anomalies, review and tune detection rules.
    - **Quarterly**: Threat model review and update, IAM access review (verify all access is still needed — remove stale access), tabletop incident response exercise, security training session, compliance evidence review.
    - **Annually**: External penetration test, full compliance audit (if applicable), security architecture review, disaster recovery / incident response full exercise, third-party vendor security review.

37. **Design third-party and vendor security management.** Your security posture is only as strong as your weakest vendor:
    - **Vendor assessment**: Before integrating a third-party service that handles your data, assess: SOC 2 report (Type II preferred), security certifications, breach history, data handling practices, data residency, and contractual security commitments.
    - **Ongoing monitoring**: Re-assess critical vendors annually. Monitor for vendor security incidents. Maintain an inventory of all third-party services with: data shared, access level, contract expiry, and security assessment date.
    - **Data sharing minimization**: Share the minimum data necessary with third parties. If a vendor needs analytics, send anonymized data, not raw PII.
    - **Contractual controls**: Include security requirements in vendor contracts: data protection standards, breach notification requirements (must notify you within 24-72 hours), right to audit, data deletion upon contract termination.
    - **Vendor exit strategy**: For critical vendors, have a documented plan for migrating away if the vendor is compromised, breached, or becomes unreliable. Know where your data resides and how to export/delete it.