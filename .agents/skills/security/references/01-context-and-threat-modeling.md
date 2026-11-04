# Security Context, Asset Identification, and Threat Modeling

This reference covers **Phase 1: Security Context and Asset Identification** and **Phase 2: Threat Modeling**. See the main SKILL.md for the phase summaries and links.

---

## Phase 1: Security Context and Asset Identification

1. **Identify the system and its security-relevant context.** Before any security analysis, establish what is being secured and why it matters. If the user has not clearly stated this, ask: "What system or component are we securing, what data does it handle, and what is the impact if it is compromised?" Do not design security controls without understanding the system's purpose and value.

   Establish the following:
   - **System description**: What the system does, its architecture (monolith, microservices, serverless), and its technology stack.
   - **Data classification**: What types of data does the system process, store, and transmit? Classify each data type:
     - **Public**: Data intended for public consumption (marketing content, public API docs). No confidentiality requirement.
     - **Internal**: Data not intended for public access but not highly sensitive (internal documentation, non-sensitive configuration). Moderate confidentiality.
     - **Confidential**: Business-sensitive data (customer lists, financial reports, proprietary algorithms, internal communications). High confidentiality, access restricted to authorized personnel.
     - **Restricted/Highly Sensitive**: Data whose exposure would cause severe harm (PII, PHI, payment card data, credentials, encryption keys, trade secrets). Highest confidentiality, strictest controls, regulatory obligations.
   - **Regulatory and compliance context**: Which regulations apply? (GDPR, HIPAA, PCI-DSS, SOC 2, FedRAMP, CCPA, industry-specific regulations.) Each regulation imposes specific, non-negotiable security controls.
   - **User and access context**: Who accesses the system? (End users, administrators, internal services, third-party integrations, partner systems.) What are their trust levels?
   - **Deployment context**: Where does the system run? (AWS, GCP, Azure, on-premise, hybrid.) What is the network topology? What existing security infrastructure is in place (WAF, SIEM, IdP)?
   - **Team context**: Team size, security expertise, security-dedicated staff (or lack thereof), security tooling budget.

2. **Identify the crown jewels.** Every system has assets that, if compromised, would cause the most damage. Identify them explicitly:
   - **Data assets**: Customer PII, financial records, health records, credentials/secrets, proprietary business data, intellectual property.
   - **System assets**: Authentication infrastructure, payment processing, admin interfaces, data pipelines, backup systems.
   - **Reputational assets**: Customer trust, regulatory standing, brand integrity.
   - **Availability assets**: Revenue-generating services, customer-facing APIs, critical infrastructure.

   Rank these assets by impact of compromise (confidentiality breach, integrity violation, availability loss). This ranking drives the prioritization of all subsequent security investments — protect the crown jewels first and most aggressively.

---

## Phase 2: Threat Modeling

3. **Conduct structured threat modeling.** Threat modeling is the foundation of security engineering. Without it, security controls are guesses. For every system or significant feature, perform a structured analysis:

   **Step 3a: Decompose the system into components.** Create a data flow diagram (DFD) showing:
   - External entities (users, third-party systems, external APIs).
   - Processes (application services, background workers, scheduled jobs).
   - Data stores (databases, caches, file systems, message queues).
   - Data flows (network connections between components, with protocol and data type).
   - Trust boundaries (lines separating zones of different trust: internet → DMZ → application tier → data tier, or tenant A → tenant B).

   **Step 3b: Apply STRIDE to each component and data flow.** For each element in the DFD, systematically consider:
   - **Spoofing** (identity): Can an attacker pretend to be a legitimate user, service, or system? (Relevant to: processes, external entities.)
   - **Tampering** (integrity): Can an attacker modify data in transit or at rest without detection? (Relevant to: data flows, data stores.)
   - **Repudiation** (accountability): Can an attacker deny performing an action because there is no audit trail? (Relevant to: processes.)
   - **Information Disclosure** (confidentiality): Can an attacker access data they should not see? (Relevant to: data flows, data stores, processes.)
   - **Denial of Service** (availability): Can an attacker make the system unavailable to legitimate users? (Relevant to: processes, data stores, data flows.)
   - **Elevation of Privilege** (authorization): Can an attacker gain higher privileges than they should have? (Relevant to: processes.)

   **Step 3c: Enumerate specific threats.** For each STRIDE category that applies, document concrete attack scenarios:
   - Threat ID (e.g., T-001).
   - Threat description: "An external attacker exploits SQL injection in the search endpoint to extract customer PII from the database."
   - Attack vector: How the attack would be carried out.
   - Affected component: Which component is targeted.
   - Impact: What happens if the attack succeeds (data breach, service outage, financial loss, regulatory penalty).
   - Likelihood: How likely is this attack given the current exposure? (Consider: is the component internet-facing? Is the vulnerability class common? Are there known exploit tools?)

   **Step 3d: Assess risk.** For each threat, assign a risk level using a simple framework:
   - **Risk = Likelihood × Impact.**
   - Use a 3-level scale for each: Low, Medium, High. Combine into a risk matrix:
     | | Low Impact | Medium Impact | High Impact |
     |---|---|---|---|
     | High Likelihood | Medium | High | Critical |
     | Medium Likelihood | Low | Medium | High |
     | Low Likelihood | Low | Low | Medium |
   - Prioritize threats: address Critical and High risks first. Accept Low risks with documentation. Medium risks are addressed based on available resources.

   **Step 3e: Define mitigations.** For each threat rated Medium or above, define one or more security controls that reduce the risk to an acceptable level. Map each mitigation to the specific threat it addresses. This mapping is the core output of threat modeling — it ensures every security control exists for a reason and every significant threat has a control.

4. **Define the trust boundaries.** Trust boundaries are where data crosses from one security zone to another. Every trust boundary requires validation, authentication, and authorization:
   - **Internet → Application**: All input is untrusted. Validate, sanitize, authenticate.
   - **Application → Database**: Application enforces access control. Database enforces least-privilege access.
   - **Service → Service**: Each service authenticates the caller and validates authorization. Even in internal networks, do not assume trust (zero-trust principle).
   - **Tenant → Tenant**: Data and compute isolation between tenants. One tenant must never be able to access another tenant's data or affect their service quality.
   - **User → Admin**: Administrative functions require elevated authentication and additional authorization checks.
   - **Production → Non-production**: Production data must not leak to non-production environments without anonymization. Non-production credentials must not work in production.

   At every trust boundary, explicitly state: what validation occurs, what authentication is required, what authorization is checked, and what logging captures the crossing.