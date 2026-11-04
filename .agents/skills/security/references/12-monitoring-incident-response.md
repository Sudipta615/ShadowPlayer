# Security Monitoring, Detection, and Incident Response

This reference covers **Phase 13: Security Monitoring, Detection, and Incident Response**. See the main SKILL.md for the phase summary and link.

---

30. **Design security monitoring and detection.** Prevention will eventually fail — detection determines how quickly you contain a breach:

    **Security event collection**:
    - **Application logs**: Authentication events (success, failure, MFA), authorization failures, input validation failures, rate limit triggers, error rates, suspicious user behavior (see the authentication skill).
    - **Infrastructure logs**: Cloud audit logs (CloudTrail, Cloud Audit Logs), VPC flow logs, DNS query logs, load balancer access logs, WAF logs, Kubernetes audit logs.
    - **System logs**: OS-level logs (auth.log, syslog), container runtime logs, host intrusion detection logs.
    - **Network logs**: Firewall logs, IDS/IPS alerts, DNS query logs, proxy logs.
    - Centralize all security-relevant logs in a SIEM (Security Information and Event Management) or log aggregation platform: Splunk, Elastic SIEM, Datadog Security, AWS Security Lake, Google Chronicle, or open-source (Wazuh + ELK).

    **Detection rules** — design alerts for:
    - **Authentication anomalies**: Credential stuffing (high-volume login failures across many accounts), brute force (many failures on single account), impossible travel, successful login after many failures (potential compromise), login from Tor/VPN exit nodes (if unusual for the user base).
    - **Authorization anomalies**: Repeated 403 errors from a single user/IP (probing for accessible resources), successful access to admin endpoints from non-admin users (privilege escalation), cross-tenant data access attempts.
    - **Data exfiltration indicators**: Unusually large API responses, high-volume data export requests, database queries returning abnormally large result sets, large outbound network transfers to unfamiliar destinations.
    - **Infrastructure anomalies**: New IAM roles or policies created outside of IaC pipeline, security group changes allowing broader access, public S3 buckets created, root account usage, API calls from unusual geographic regions, EC2/VM instances launched in unusual regions.
    - **Application anomalies**: Spike in error rates, new endpoints being accessed (scanning), SQL error patterns in logs (injection attempts), file access patterns indicating path traversal attempts.
    - **Runtime anomalies**: Unexpected processes in containers, outbound connections to known malicious IPs, cryptomining indicators (high CPU on idle services), shell execution in application containers.

    **Alert prioritization**:
    - **P1 (immediate response)**: Confirmed data breach, active exploitation, credential compromise, unauthorized infrastructure access.
    - **P2 (respond within 1 hour)**: Likely attack in progress (high-confidence detection rules), security control failure (WAF down, SIEM gap), compromised service account.
    - **P3 (respond within business day)**: Suspicious activity requiring investigation, vulnerability with active exploit published, configuration drift detected.
    - **P4 (informational)**: Low-confidence anomalies, trend analysis, compliance drift.

31. **Design the incident response plan.** When a security incident occurs, the response must be structured and practiced, not improvised:

    **Incident response phases**:

    **Phase 1: Detection and Triage (0-30 minutes)**:
    - Confirm the incident is real (not a false positive).
    - Classify severity: data breach, service compromise, credential compromise, denial of service, policy violation.
    - Assign an Incident Commander (IC) who owns the response.
    - Open a dedicated communication channel (Slack channel, war room).
    - Begin a timeline log documenting every action taken and every finding.

    **Phase 2: Containment (30 minutes - 4 hours)**:
    - **Immediate containment**: Stop the active threat without destroying evidence.
      - Compromise of service: Isolate the compromised service (network isolation, revoke its credentials, remove from load balancer). Do not destroy the instance — it is evidence.
      - Compromised credentials: Rotate all potentially compromised credentials immediately. Force re-authentication for all affected users.
      - Data exfiltration: Block the exfiltration channel (IP block, revoke API key, disable the compromised account).
      - Ongoing attack: Engage DDoS protection, enable additional WAF rules, rate-limit aggressively.
    - **Evidence preservation**: Capture: disk snapshots of compromised instances, memory dumps (if possible), log exports, network captures, container images. Preserve in a secure, read-only location.

    **Phase 3: Investigation (hours - days)**:
    - Determine: initial attack vector (how did the attacker get in?), scope of compromise (what systems/data were accessed?), attacker actions (what did they do — lateral movement, data access, persistence mechanisms?), data impact (what data was exposed, modified, or exfiltrated?).
    - Use logs, audit trails, and forensic artifacts to reconstruct the attacker's timeline.
    - Identify all affected systems and credentials. Assume lateral movement until proven otherwise.

    **Phase 4: Eradication and Recovery (days)**:
    - Remove all attacker access: close the initial vulnerability, remove any backdoors or persistence mechanisms, rotate all credentials that may have been exposed.
    - Rebuild compromised systems from clean images (never "clean" a compromised system — rebuild it).
    - Restore from backups if data integrity is in question (verify backup integrity first).
    - Implement additional monitoring for the specific attack pattern.

    **Phase 5: Post-Incident (1-2 weeks)**:
    - **Post-incident review (blameless)**: What happened? What went well? What could be improved? What systemic issues enabled the incident?
    - **Action items**: Prioritized list of security improvements to prevent recurrence. Assign owners and deadlines.
    - **Communication**: Notify affected parties as required by regulations (GDPR: 72 hours to supervisory authority, HIPAA: 60 days to affected individuals). Prepare customer communication if data was breached.
    - **Update documentation**: Update threat model, runbooks, detection rules, and incident response plan based on lessons learned.

    **Incident response readiness**:
    - Document the incident response plan and ensure all relevant team members know where to find it and what their roles are.
    - Conduct tabletop exercises quarterly: present a realistic incident scenario and walk through the response process. Identify gaps in procedures, tooling, and team readiness.
    - Maintain a contact list for the incident response team, including after-hours contact methods.
    - Pre-authorize emergency actions: the IC should have pre-authorized authority to isolate systems, revoke credentials, and shut down services without requiring management approval during an active incident.