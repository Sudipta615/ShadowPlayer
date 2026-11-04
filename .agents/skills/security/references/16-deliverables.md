# Security Architecture Output and Deliverables

This reference covers **Phase 17: Security Architecture Output and Deliverables**. See the main SKILL.md for the phase summary and link.

---

38. **Produce security architecture deliverables.** At the conclusion of every security design engagement, produce:

    - **Security architecture summary**: A concise document stating the system's security context, data classification, threat model summary, and the security architecture decisions.
    - **Threat model**: Complete STRIDE analysis with system decomposition, trust boundaries, identified threats, risk ratings, and mapped mitigations. Reference specific threats by ID when justifying controls.
    - **Security controls inventory**: A comprehensive table listing every security control implemented, the threat(s) it mitigates, the implementation technology, the responsible team, and the verification method (test, scan, audit).
    - **Data flow diagram with trust boundaries**: Visual representation of how data moves through the system, where trust boundaries exist, and what security controls are applied at each boundary.
    - **Encryption architecture**: What data is encrypted, at which layers (transport, storage, application), with which algorithms and key management approach.
    - **Secrets management architecture**: Where secrets are stored, how they are distributed, rotation schedules, and revocation procedures.
    - **Network security diagram**: VPC topology, subnet layout, security group rules, egress controls, and connectivity to external systems.
    - **CI/CD security pipeline design**: Pipeline stages with security gates, tools used at each stage, and blocking vs. non-blocking policies.
    - **Incident response plan**: Roles, communication channels, response procedures, escalation paths, and evidence collection procedures.
    - **Compliance mapping**: For each applicable compliance requirement, the specific engineering control that addresses it and the evidence collected.
    - **Security testing plan**: Testing types, tools, frequency, scope, and remediation SLAs.
    - **Security metrics and dashboard design**: Which metrics to track, alerting thresholds, and reporting cadence.
    - **Risk register**: Open security risks, accepted risks with justification and compensating controls, and a remediation roadmap.
    - **ADRs for security decisions**: For each significant security architecture decision (technology choice, accepted risk, security tradeoff), a decision record with context, decision, alternatives considered, and consequences.
    - **Open questions**: Areas requiring further threat analysis, stakeholder input, compliance clarification, or third-party assessment.